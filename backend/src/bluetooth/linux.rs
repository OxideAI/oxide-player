//! Linux Bluetooth implementation using the `bluer` crate (v0.17).
//!
//! bluer 0.17's `DeviceEvent` and `DeviceProperty` types are not re‑exported,
//! so we rely on the adapter‑level API for device enumeration and manage
//! connection/paired state locally.  The in‑memory cache is populated from
//! `adapter.device_addresses()` and updated when operations succeed.

use crate::bluetooth::input::BluetoothInputManager;
use crate::bluetooth::types::{BtDevice, BtEvent, BtEventKind};
use anyhow::{Context, Result};
use bluer::{Adapter, AdapterEvent, Address, Device};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use futures_util::StreamExt;
use tokio::sync::{broadcast, Mutex, RwLock};

/// Manages the Bluetooth device lifecycle via BlueZ D‑Bus.
#[derive(Clone)]
pub struct BluetoothManager {
    inner: Arc<Inner>,
}

struct Inner {
    session: Mutex<Option<bluer::Session>>,
    adapter_name: RwLock<Option<String>>,
    /// Address → BtDevice.  Populated from `adapter.device_addresses()`.
    devices: RwLock<HashMap<String, BtDevice>>,
    event_tx: broadcast::Sender<BtEvent>,
    discovery_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    _event_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    input: BluetoothInputManager,
}

impl BluetoothManager {
    /// Create a new `BluetoothManager`.  Best‑effort: if BlueZ is unavailable
    /// the manager stays usable — all device operations return clear errors.
    pub async fn new() -> Self {
        let (event_tx, _) = broadcast::channel(32);
        let mgr = BluetoothManager {
            inner: Arc::new(Inner {
                session: Mutex::new(None),
                adapter_name: RwLock::new(None),
                devices: RwLock::new(HashMap::new()),
                event_tx,
                discovery_handle: Mutex::new(None),
                _event_task: Mutex::new(None),
                input: BluetoothInputManager::new(),
            }),
        };

        if let Err(e) = mgr.init().await {
            tracing::warn!("Bluetooth not available (Bluetooth disabled or no adapter): {e}");
        }

        mgr
    }

    // ── internal helpers ──────────────────────────────────────────────

    async fn init(&self) -> Result<()> {
        let session =
            bluer::Session::new().await.context("create bluer D‑Bus session")?;

        let names = session.adapter_names().await?;
        let adapter_name = names
            .first()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter found"))?;

        let adapter = session.adapter(&adapter_name)
            .with_context(|| format!("get adapter '{adapter_name}'"))?;

        if !adapter.is_powered().await.unwrap_or(false) {
            adapter.set_powered(true).await.context("power on Bluetooth adapter")?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Seed the cache from adapter device list.
        self.sync_device_cache(&adapter).await;

        *self.inner.session.lock().await = Some(session);
        *self.inner.adapter_name.write().await = Some(adapter_name);

        // Spawn the 5‑s poll loop so the cache stays reasonably fresh.
        let poller = self.clone();
        let handle = tokio::spawn(async move { poller.event_listener().await });
        *self.inner._event_task.lock().await = Some(handle);

        tracing::info!("Bluetooth subsystem initialised");
        Ok(())
    }

    /// Re-populate the device cache from `adapter.device_addresses()`.
    /// Fetches device properties (name, alias, class, icon, RSSI) for each device.
    async fn sync_device_cache(&self, adapter: &Adapter) {
        let addrs = match adapter.device_addresses().await {
            Ok(a) => a,
            Err(_) => return,
        };

        let mut cache = self.inner.devices.write().await;
        for addr in &addrs {
            let addr_str = addr.to_string();
            // Get device handle to fetch properties
            let device = match adapter.device(*addr) {
                Ok(d) => d,
                Err(_) => {
                    cache.entry(addr_str).or_insert_with(|| BtDevice {
                        address: addr.to_string(),
                        name: None,
                        alias: None,
                        class: None,
                        icon: None,
                        rssi: None,
                        connected: false,
                        paired: false,
                        trusted: false,
                    });
                    continue;
                }
            };

            // Fetch all relevant properties
            let name = device.name().await.ok().flatten();
            let alias = device.alias().await.ok().and_then(|s| Some(s));
            let class = device.class().await.ok().flatten();
            let icon = device.icon().await.ok().flatten();
            let rssi = device.rssi().await.ok().flatten();

            cache.entry(addr_str).and_modify(|d| {
                d.name = name.clone();
                d.alias = alias.clone();
                d.class = class;
                d.icon = icon.clone();
                d.rssi = rssi;
            }).or_insert(BtDevice {
                address: addr.to_string(),
                name,
                alias,
                class,
                icon,
                rssi,
                connected: false,
                paired: false,
                trusted: false,
            });
        }
    }

    /// Create an adapter handle from the stored session.
    async fn adapter(&self) -> Result<Adapter> {
        let guard = self.inner.session.lock().await;
        let session = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Bluetooth session not initialised"))?;
        let name = self.inner.adapter_name.read().await.clone()
            .ok_or_else(|| anyhow::anyhow!("no Bluetooth adapter configured"))?;
        session.adapter(&name)
            .with_context(|| format!("get adapter '{name}'"))
    }

    /// Get a `Device` handle by address.
    async fn get_device(&self, address: &str) -> Result<Device> {
        let adapter = self.adapter().await?;
        let addr: Address = address
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid Bluetooth address '{address}': {e}"))?;
        adapter
            .device(addr)
            .with_context(|| format!("device '{address}' not found on adapter"))
    }

    fn emit(&self, event: BtEvent) {
        let _ = self.inner.event_tx.send(event);
    }

    // ── background tasks ─────────────────────────────────────────────

    /// Periodically re-sync the cache so removal of devices (e.g. via
    /// `bluetoothctl`) is eventually reflected in the frontend.
    async fn event_listener(&self) {
        loop {
            if let Ok(adapter) = self.adapter().await {
                self.sync_device_cache(&adapter).await;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }

    // ── public API ───────────────────────────────────────────────────

    // -- discovery --

    pub async fn start_discovery(&self, timeout_secs: u32) -> Result<()> {
        self.stop_discovery().await;

        let adapter = self.adapter().await?;
        let mut stream = adapter
            .discover_devices()
            .await
            .context("start Bluetooth device discovery")?;

        let mgr = self.clone();
        let handle = tokio::spawn(async move {
            let deadline =
                tokio::time::Instant::now() + Duration::from_secs(timeout_secs.max(5) as u64);

            loop {
                tokio::select! {
                    event = stream.next() => {
                        match event {
                            Some(AdapterEvent::DeviceAdded(addr)) => {
                                mgr.on_device_found(&adapter, addr).await;
                            }
                            Some(AdapterEvent::DeviceRemoved(addr)) => {
                                mgr.on_device_lost(addr).await;
                            }
                            Some(AdapterEvent::PropertyChanged(_)) => {}
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

    pub async fn stop_discovery(&self) {
        if let Some(h) = self.inner.discovery_handle.lock().await.take() {
            h.abort();
        }
    }

    pub async fn is_discovering(&self) -> bool {
        self.inner.discovery_handle.lock().await.is_some()
    }

    // -- pairing / bonding --

    pub async fn pair(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device.pair().await.with_context(|| format!("pair with {address}"))?;

        // Mark cached paired.
        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.paired = true;
        }

        self.emit(BtEvent {
            kind: BtEventKind::Paired,
            device: self.device_or_placeholder(address).await,
        });
        Ok(())
    }

    // -- connection --

    pub async fn connect(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device.connect().await.with_context(|| format!("connect to {address}"))?;

        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.connected = true;
        }

        self.emit(BtEvent {
            kind: BtEventKind::Connected,
            device: self.device_or_placeholder(address).await,
        });
        Ok(())
    }

    pub async fn connect_profile(&self, _address: &str, _profile: &str) -> Result<()> {
        anyhow::bail!("connect_profile is not yet implemented");
    }

    pub async fn disconnect(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device.disconnect().await.with_context(|| format!("disconnect {address}"))?;

        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.connected = false;
        }

        self.emit(BtEvent {
            kind: BtEventKind::Disconnected,
            device: self.device_or_placeholder(address).await,
        });
        Ok(())
    }

    pub async fn forget(&self, address: &str) -> Result<()> {
        let adapter = self.adapter().await?;
        let addr: Address = address.parse()
            .map_err(|e| anyhow::anyhow!("invalid Bluetooth address '{address}': {e}"))?;
        adapter.remove_device(addr).await
            .with_context(|| format!("forget (remove) {address}"))?;

        self.inner.devices.write().await.remove(address);
        Ok(())
    }

    // -- device management --

    /// Set a user-friendly alias (name) for a paired device.
    pub async fn set_alias(&self, address: &str, name: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        device.set_alias(name.to_string()).await
            .with_context(|| format!("set alias for {address}"))?;

        // Update cache
        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.alias = Some(name.to_string());
        }

        self.emit(BtEvent {
            kind: BtEventKind::Paired, // Reuse Paired event for cache update
            device: self.device_or_placeholder(address).await,
        });
        Ok(())
    }

    /// Test connectivity to a device by connecting and then disconnecting.
    pub async fn test_connectivity(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        
        // Try to connect
        device.connect().await
            .with_context(|| format!("test connect to {address}"))?;
        
        // Update cache temporarily
        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.connected = true;
        }
        
        // Brief pause to let connection stabilize
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Disconnect
        device.disconnect().await
            .with_context(|| format!("test disconnect from {address}"))?;
        
        // Update cache
        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.connected = false;
        }
        
        Ok(())
    }

    /// Wake and connect to a paired Bluetooth device that may be in sleep/standby mode.
    /// 
    /// Many Bluetooth speakers go into deep sleep after being idle. This method
    /// attempts to connect with retry logic and delays to give the device time
    /// to wake up and respond.
    /// 
    /// The algorithm:
    /// 1. First attempt to connect immediately (may wake the device)
    /// 2. If that fails, wait 2 seconds and retry (device may be waking up)
    /// 3. If that fails, wait 5 seconds and retry one more time
    /// 4. On success, update the cache and emit Connected event
    pub async fn wake_and_connect(&self, address: &str) -> Result<()> {
        let device = self.get_device(address).await?;
        
        // Attempt 1: immediate connect (may wake the device)
        let mut last_err = match device.connect().await {
            Ok(()) => {
                // Success on first try!
                if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
                    entry.connected = true;
                }
                self.emit(BtEvent {
                    kind: BtEventKind::Connected,
                    device: self.device_or_placeholder(address).await,
                });
                return Ok(());
            }
            Err(e) => e,
        };
        
        // Attempt 2: wait 2 seconds, then retry
        tracing::info!("Wake attempt 1 failed for {address}, waiting 2s before retry: {last_err}");
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        last_err = match device.connect().await {
            Ok(()) => {
                if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
                    entry.connected = true;
                }
                self.emit(BtEvent {
                    kind: BtEventKind::Connected,
                    device: self.device_or_placeholder(address).await,
                });
                return Ok(());
            }
            Err(e) => e,
        };
        
        // Attempt 3: wait 5 seconds, then final retry
        tracing::info!("Wake attempt 2 failed for {address}, waiting 5s before final retry: {last_err}");
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        device.connect().await
            .with_context(|| format!("wake and connect to {address} after retries"))?;
        
        if let Some(entry) = self.inner.devices.write().await.get_mut(address) {
            entry.connected = true;
        }
        
        self.emit(BtEvent {
            kind: BtEventKind::Connected,
            device: self.device_or_placeholder(address).await,
        });
        
        Ok(())
    }

    // -- queries --

    /// Check whether Bluetooth is available (adapter reachable).
    /// Returns `Ok(())` if the adapter can be accessed, or an error
    /// describing why Bluetooth is unavailable.
    pub async fn check_available(&self) -> Result<()> {
        self.adapter().await?;
        Ok(())
    }

    pub async fn list_devices(&self) -> Vec<BtDevice> {
        // Re-sync so we pick up devices paired via external tools (bluetoothctl).
        if let Ok(adapter) = self.adapter().await {
            self.sync_device_cache(&adapter).await;
        }
        self.inner.devices.read().await
            .values()
            .cloned()
            .collect()
    }

    pub async fn connected_devices(&self) -> Vec<BtDevice> {
        self.inner
            .devices
            .read()
            .await
            .values()
            .filter(|d| d.connected)
            .cloned()
            .collect()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BtEvent> {
        self.inner.event_tx.subscribe()
    }

    pub fn input(&self) -> BluetoothInputManager {
        self.inner.input.clone()
    }

    // -- helper --

    async fn device_or_placeholder(&self, address: &str) -> BtDevice {
        self.inner
            .devices
            .read()
            .await
            .get(address)
            .cloned()
            .unwrap_or_else(|| BtDevice {
                address: address.to_string(),
                name: None,
                alias: None,
                class: None,
                icon: None,
                rssi: None,
                connected: false,
                paired: false,
                trusted: false,
            })
    }

    // -- event callbacks (called from the discovery task) --

    async fn on_device_found(&self, adapter: &Adapter, addr: Address) {
        let addr_str = addr.to_string();
        {
            let mut cache = self.inner.devices.write().await;
            if !cache.contains_key(&addr_str) {
                // Fetch device properties for newly discovered device
                let device = match adapter.device(addr) {
                    Ok(d) => d,
                    Err(_) => {
                        cache.insert(addr_str.clone(), BtDevice {
                            address: addr_str.clone(),
                            name: None,
                            alias: None,
                            class: None,
                            icon: None,
                            rssi: None,
                            connected: false,
                            paired: false,
                            trusted: false,
                        });
                        self.emit(BtEvent {
                            kind: BtEventKind::DeviceFound,
                            device: self.device_or_placeholder(&addr_str).await,
                        });
                        return;
                    }
                };

                let name = device.name().await.ok().flatten();
                let alias = device.alias().await.ok().and_then(|s| Some(s));
                let class = device.class().await.ok().flatten();
                let icon = device.icon().await.ok().flatten();
                let rssi = device.rssi().await.ok().flatten();

                cache.insert(addr_str.clone(), BtDevice {
                    address: addr_str.clone(),
                    name,
                    alias,
                    class,
                    icon,
                    rssi,
                    connected: false,
                    paired: false,
                    trusted: false,
                });
            }
        }

        self.emit(BtEvent {
            kind: BtEventKind::DeviceFound,
            device: self.device_or_placeholder(&addr_str).await,
        });
    }

    async fn on_device_lost(&self, addr: Address) {
        let addr_str = addr.to_string();
        if let Some(d) = self.inner.devices.write().await.remove(&addr_str) {
            self.emit(BtEvent {
                kind: BtEventKind::DeviceLost,
                device: d,
            });
        }
    }
}
