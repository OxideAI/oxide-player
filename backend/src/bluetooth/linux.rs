use crate::bluetooth::input::BluetoothInputManager;
use crate::bluetooth::types::{BtDevice, BtEvent, BtEventKind};
use anyhow::{Context, Result};
use bluer::{Adapter, AdapterEvent, Address, Device, DeviceEvent, Session};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio_stream::StreamExt;

/// Manages the Bluetooth device lifecycle via BlueZ D‑Bus.
///
/// Owns a [`bluer::Session`] and the first available adapter. Provides async
/// methods for discovery, pairing, connection, disconnection, and state
/// monitoring. Runs a background task that refreshes device state every 5 s
/// and publishes [`BtEvent`]s on a broadcast channel.
#[derive(Clone)]
pub struct BluetoothManager {
    inner: Arc<Inner>,
}

struct Inner {
    session: Mutex<Option<Session>>,
    adapter_name: RwLock<Option<String>>,
    devices: RwLock<Vec<BtDevice>>,
    event_tx: broadcast::Sender<BtEvent>,
    discovery_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    _event_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    input: BluetoothInputManager,
}

impl BluetoothManager {
    /// Create a new `BluetoothManager` and attempt to initialise the BlueZ
    /// session and adapter. Best‑effort: if BlueZ is unreachable or no adapter
    /// is available, the manager is still usable — all device operations will
    /// return clear errors.
    pub async fn new() -> Self {
        let (event_tx, _) = broadcast::channel(32);
        let mgr = BluetoothManager {
            inner: Arc::new(Inner {
                session: Mutex::new(None),
                adapter_name: RwLock::new(None),
                devices: RwLock::new(Vec::new()),
                event_tx,
                discovery_handle: Mutex::new(None),
                _event_task: Mutex::new(None),
                input: BluetoothInputManager::new(),
            }),
        };

        // Best‑effort initialisation — don't block startup if Bluetooth is
        // absent (CI, headless server without BT hardware, …).
        if let Err(e) = mgr.init().await {
            tracing::warn!("Bluetooth not available (Bluetooth disabled or no adapter): {e}");
        }

        mgr
    }

    // ── internal helpers ──────────────────────────────────────────────

    /// Try to connect to BlueZ, find the first adapter, power it on, read
    /// existing paired devices, and start the event‑listener background task.
    async fn init(&self) -> Result<()> {
        let session = Session::new()
            .await
            .context("create bluer D‑Bus session")?;

        let names = session
            .adapter_names()
            .await
            .context("enumerate Bluetooth adapters")?;
        let adapter_name = names
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter found"))?;

        let adapter = session
            .adapter(&adapter_name)
            .await
            .with_context(|| format!("get adapter '{adapter_name}'"))?;

        // Ensure the adapter is powered on.
        if !adapter.is_powered().await.unwrap_or(false) {
            adapter
                .set_powered(true)
                .await
                .context("power on Bluetooth adapter")?;
            // Give BlueZ a moment to settle.
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Snapshot currently known (paired) devices.
        let devices = Self::collect_known_devices(&adapter).await;

        *self.inner.session.lock().await = Some(session);
        *self.inner.adapter_name.write().await = Some(adapter_name);
        *self.inner.devices.write().await = devices;

        // Spawn the connection‑state monitor.
        let monitor = self.clone();
        let handle = tokio::spawn(async move {
            monitor.event_listener().await;
        });
        *self.inner._event_task.lock().await = Some(handle);

        tracing::info!("Bluetooth subsystem initialised");
        Ok(())
    }

    /// Create a short‑lived adapter handle from the stored session.
    async fn adapter(&self) -> Result<Adapter> {
        let guard = self.inner.session.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Bluetooth session not initialised"))?;
        let name = self
            .inner
            .adapter_name
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter configured"))?;
        // Create the adapter while holding the lock; the Adapter is
        // self‑contained (D‑Bus object path) and outlives the guard.
        let adapter = session
            .adapter(&name)
            .await
            .with_context(|| format!("get adapter '{name}'"))?;
        Ok(adapter)
    }

    /// Get a device handle by address, returning an error if BlueZ doesn't
    /// know the device (never seen / already removed).
    async fn get_device(&self, address: &str) -> Result<Device> {
        let adapter = self.adapter().await?;
        let addr: Address = address
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid Bluetooth address '{address}': {e}"))?;
        adapter
            .device(addr)
            .await
            .with_context(|| format!("device '{address}' not found on adapter"))
    }

    /// Convert a [`bluer::Device`] into our portable [`BtDevice`].
    async fn bt_device(device: &Device) -> BtDevice {
        // Every `unwrap_or` is a safe fallback for values BlueZ may report as
        // unavailable (e.g. RSSI of an idle device).
        BtDevice {
            address: device.address().await.unwrap_or_default().to_string(),
            name: device.name().await.unwrap_or(None),
            rssi: device.rssi().await.unwrap_or(None),
            connected: device.is_connected().await.unwrap_or(false),
            paired: device.is_paired().await.unwrap_or(false),
            trusted: device.is_trusted().await.unwrap_or(false),
        }
    }

    /// Read all known devices from the adapter and convert to BtDevice.
    async fn collect_known_devices(adapter: &Adapter) -> Vec<BtDevice> {
        let mut devices = Vec::new();
        if let Ok(addrs) = adapter.device_addresses().await {
            for addr in addrs {
                if let Ok(device) = adapter.device(addr).await {
                    let bt = Self::bt_device(&device).await;
                    devices.push(bt);
                }
            }
        }
        devices
    }

    /// Refresh the in‑memory device cache by re‑reading every known device
    /// from BlueZ, and publish connect/disconnect events for transitions.
    async fn refresh_devices(&self) {
        let adapter = match self.adapter().await {
            Ok(a) => a,
            Err(_) => return,
        };

        let fresh = Self::collect_known_devices(&adapter).await;

        let mut devices = self.inner.devices.write().await;
        // Detect transitions from the previous snapshot.
        for fresh_dev in &fresh {
            if let Some(prev) = devices.iter().find(|d| d.address == fresh_dev.address) {
                if prev.connected != fresh_dev.connected {
                    let kind = if fresh_dev.connected {
                        BtEventKind::Connected
                    } else {
                        BtEventKind::Disconnected
                    };
                    let _ = self.inner.event_tx.send(BtEvent {
                        kind,
                        device: fresh_dev.clone(),
                    });
                }
            }
        }
        *devices = fresh;
    }

    /// Publish an event on the broadcast channel (fire‑and‑forget).
    fn emit(&self, event: BtEvent) {
        let _ = self.inner.event_tx.send(event);
    }

    // ── background tasks ─────────────────────────────────────────────

    /// Periodically refresh device state so the UI stays up to date even
    /// without explicit adapter‑event subscriptions.
    async fn event_listener(&self) {
        loop {
            self.refresh_devices().await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    // ── public API ───────────────────────────────────────────────────

    // -- discovery --

    /// Start scanning for nearby Bluetooth audio devices.
    ///
    /// Automatically stops after `timeout_secs` seconds (default 15).
    /// Results are added to the manager's internal cache and published as
    /// [`BtEventKind::DeviceFound`] events.
    pub async fn start_discovery(&self, timeout_secs: u32) -> Result<()> {
        // Cancel any in‑flight discovery first.
        self.stop_discovery().await;

        let adapter = self.adapter()?; // already mutex-guarded
        let mut stream = adapter
            .discover_devices()
            .await
            .context("start Bluetooth device discovery")?;

        let mgr = self.clone();
        let handle = tokio::spawn(async move {
            let deadline = tokio::time::Instant::now()
                + Duration::from_secs(timeout_secs.max(5) as u64);

            loop {
                tokio::select! {
                    event = stream.next() => {
                        match event {
                            Some(DeviceEvent::Added(addr)) => mgr.on_device_found(addr).await,
                            Some(DeviceEvent::Lost(addr)) => mgr.on_device_lost(addr).await,
                            Some(DeviceEvent::PropertyChanged(addr)) => {
                                mgr.on_device_changed(addr).await
                            }
                            None => break,
                        }
                    }
                    _ = tokio::time::sleep_until(deadline) => {
                        tracing::debug!("Bluetooth discovery auto‑timeout ({timeout_secs}s)");
                        break;
                    }
                }
            }
            tracing::debug!("Bluetooth discovery stopped");
        });

        *self.inner.discovery_handle.lock().await = Some(handle);
        Ok(())
    }

    /// Abort an active discovery scan.
    pub async fn stop_discovery(&self) {
        if let Some(h) = self.inner.discovery_handle.lock().await.take() {
            h.abort();
        }
    }

    /// Whether a discovery scan is currently running.
    pub async fn is_discovering(&self) -> bool {
        self.inner.discovery_handle.lock().await.is_some()
    }

    // -- pairing / bonding --

    /// Pair (bond) with a device. The device must be discoverable / in range.
    /// After pairing the device is ready to be connected.
    pub async fn pair(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device
            .pair()
            .await
            .with_context(|| format!("pair with {address}"))?;
        self.emit(BtEvent {
            kind: BtEventKind::Paired,
            device: Self::bt_device(&device).await,
        });
        self.refresh_devices().await;
        Ok(())
    }

    // -- connection --

    /// Connect to a paired device. Establishes the A2DP profile transport.
    pub async fn connect(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device
            .connect()
            .await
            .with_context(|| format!("connect to {address}"))?;
        self.refresh_devices().await;
        Ok(())
    }

    /// Connect a specific profile (e.g. `"a2dp_sink"`) on a paired device.
    /// Useful when the default `connect()` doesn't establish the right profile.
    pub async fn connect_profile(&self, address: &str, profile: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device
            .connect_profile(profile)
            .await
            .with_context(|| format!("connect profile '{profile}' on {address}"))?;
        self.refresh_devices().await;
        Ok(())
    }

    /// Disconnect a connected device. The pairing (bond) is preserved.
    pub async fn disconnect(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device
            .disconnect()
            .await
            .with_context(|| format!("disconnect {address}"))?;
        self.refresh_devices().await;
        Ok(())
    }

    /// Remove (unpair / forget) a device. The bonding information is deleted,
    /// and pairing from scratch is required to use the device again.
    pub async fn forget(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device
            .remove()
            .await
            .with_context(|| format!("forget (remove) {address}"))?;
        self.refresh_devices().await;
        Ok(())
    }

    // -- queries --

    /// Return all known (paired) devices with their current state.
    pub async fn list_devices(&self) -> Vec<BtDevice> {
        self.inner.devices.read().await.clone()
    }

    /// Return only those devices that are currently connected.
    pub async fn connected_devices(&self) -> Vec<BtDevice> {
        self.inner
            .devices
            .read()
            .await
            .iter()
            .filter(|d| d.connected)
            .cloned()
            .collect()
    }

    /// Subscribe to real‑time device events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<BtEvent> {
        self.inner.event_tx.subscribe()
    }

    /// Access the Bluetooth A2DP Sink input manager.
    pub fn input(&self) -> BluetoothInputManager {
        self.inner.input.clone()
    }

    // -- event callbacks (called from the discovery task) --

    async fn on_device_found(&self, addr: Address) {
        let adapter = match self.adapter().await {
            Ok(a) => a,
            Err(_) => return,
        };
        let device = match adapter.device(addr).await {
            Ok(d) => d,
            Err(_) => return,
        };
        let bt = Self::bt_device(&device).await;

        // Add to cache if not already present.
        let mut devices = self.inner.devices.write().await;
        if !devices.iter().any(|d| d.address == bt.address) {
            devices.push(bt.clone());
            self.emit(BtEvent {
                kind: BtEventKind::DeviceFound,
                device: bt,
            });
        }
    }

    async fn on_device_lost(&self, addr: Address) {
        let addr_s = addr.to_string();
        let mut devices = self.inner.devices.write().await;
        if let Some(pos) = devices.iter().position(|d| d.address == addr_s) {
            let device = devices.remove(pos);
            self.emit(BtEvent {
                kind: BtEventKind::DeviceLost,
                device,
            });
        }
    }

    async fn on_device_changed(&self, addr: Address) {
        let adapter = match self.adapter().await {
            Ok(a) => a,
            Err(_) => return,
        };
        let device = match adapter.device(addr).await {
            Ok(d) => d,
            Err(_) => return,
        };
        let bt = Self::bt_device(&device).await;

        let mut devices = self.inner.devices.write().await;
        if let Some(existing) = devices.iter_mut().find(|d| d.address == bt.address) {
            let was_connected = existing.connected;
            if was_connected != bt.connected {
                let kind = if bt.connected {
                    BtEventKind::Connected
                } else {
                    BtEventKind::Disconnected
                };
                self.emit(BtEvent {
                    kind,
                    device: bt.clone(),
                });
            }
            *existing = bt;
        }
    }
}
