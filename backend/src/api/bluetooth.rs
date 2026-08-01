use crate::bluetooth::mpd_integration;
use crate::bluetooth::types::BtDevice;
use crate::devices::include_injector::IncludeInjector;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{Json, Router};
use axum::routing::{get, post};
use serde::Deserialize;

/// Wire the Bluetooth API routes into the application router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/bluetooth/devices", get(bluetooth_devices))
        .route("/api/bluetooth/devices/audio", get(bluetooth_audio_devices))
        .route("/api/bluetooth/scan", post(bluetooth_scan))
        .route("/api/bluetooth/scan/stop", post(bluetooth_scan_stop))
        .route("/api/bluetooth/scan/results", get(bluetooth_scan_results))
        .route("/api/bluetooth/pair", post(bluetooth_pair))
        .route("/api/bluetooth/connect", post(bluetooth_connect))
        .route("/api/bluetooth/wake-connect", post(bluetooth_wake_connect))
        .route("/api/bluetooth/disconnect", post(bluetooth_disconnect))
        .route("/api/bluetooth/forget", post(bluetooth_forget))
        .route("/api/bluetooth/remove-output", post(bluetooth_remove_output))
        .route("/api/bluetooth/rename", post(bluetooth_rename))
        .route("/api/bluetooth/test-connect", post(bluetooth_test_connect))
        .route("/api/bluetooth/input/enable", post(bluetooth_input_enable))
        .route("/api/bluetooth/input/disable", post(bluetooth_input_disable))
        .route("/api/bluetooth/input/status", get(bluetooth_input_status))
}

/// Ensure Bluetooth is available, returning 503 if not.
fn bt_available<T>(r: anyhow::Result<T>) -> AppResult<T> {
    r.map_err(|e| {
        let msg = e.to_string();
        if msg.contains("not supported") || msg.contains("not initialised") || msg.contains("no Bluetooth") {
            AppError::BluetoothUnavailable
        } else {
            AppError::Bluetooth(msg)
        }
    })
}

/// Ensure MPD loads managed output fragments whenever a Bluetooth output is
/// created. The installer sets `mpd_config`; source installs may intentionally
/// omit it and retain the existing manual-include warning.
async fn ensure_output_include(s: &AppState) -> AppResult<()> {
    let cfg = s.config().await;
    if let Some(path) = cfg.mpd_config {
        IncludeInjector::new(path)
            .ensure_include(s.device_configs().dir())
            .map_err(|e| AppError::Bluetooth(e.to_string()))?;
    }
    Ok(())
}

// ── request / response types ──────────────────────────────────────

#[derive(Deserialize)]
struct ScanBody {
    #[serde(default = "default_scan_timeout")]
    timeout: u32,
}

fn default_scan_timeout() -> u32 {
    15
}

#[derive(Deserialize)]
struct AddressBody {
    address: String,
}

/// Request body for renaming a Bluetooth device.
#[derive(Deserialize)]
struct RenameBody {
    address: String,
    name: String,
}

/// Request body for testing connectivity to a Bluetooth device.
#[derive(Deserialize)]
struct TestConnectBody {
    address: String,
}

/// Response for test connectivity.
#[derive(serde::Serialize)]
struct TestConnectResponse {
    success: bool,
    message: String,
}

/// Summarises the current scan state and found devices.
#[derive(serde::Serialize)]
struct ScanResultsResponse {
    active: bool,
    devices: Vec<BtDevice>,
}

/// Summarises the Bluetooth input (A2DP sink) status.
#[derive(serde::Serialize)]
struct InputStatusResponse {
    enabled: bool,
    // Whether a phone/tablet is currently streaming audio.
    // Filled by U4; always false for now.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    streaming: bool,
}

// ── handlers ───────────────────────────────────────────────────────

/// `GET /api/bluetooth/devices` — list all known (paired) BT devices.
///
/// Returns 503 when Bluetooth is not available (no adapter, no BlueZ, stub
/// platform). The frontend uses this to hide the Bluetooth section entirely
/// instead of showing a misleading "loading…" state.
async fn bluetooth_devices(State(s): State<AppState>) -> AppResult<Json<Vec<BtDevice>>> {
    bt_available(s.bluetooth().check_available().await)?;
    Ok(Json(s.bluetooth().list_devices().await))
}

/// `POST /api/bluetooth/scan` — start Bluetooth device discovery.
///
/// Body: `{ "timeout": 15 }` (optional, defaults to 15 seconds).
/// Discovered devices appear in the device cache and are returned by
/// `GET /api/bluetooth/scan/results`.
async fn bluetooth_scan(
    State(s): State<AppState>,
    Json(body): Json<ScanBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().start_discovery(body.timeout).await)?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/scan/stop` — abort an active discovery scan.
async fn bluetooth_scan_stop(State(s): State<AppState>) -> StatusCode {
    s.bluetooth().stop_discovery().await;
    StatusCode::OK
}

/// `GET /api/bluetooth/scan/results` — poll for discovery results.
async fn bluetooth_scan_results(State(s): State<AppState>) -> Json<ScanResultsResponse> {
    let bt = s.bluetooth();
    Json(ScanResultsResponse {
        active: bt.is_discovering().await,
        devices: bt.list_devices().await,
    })
}

/// `POST /api/bluetooth/pair` — pair (bond) with a discovered device.
///
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_pair(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().pair(&body.address).await)?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/connect` — connect to a paired device.
///
/// Also creates an MPD output config fragment for the speaker so it appears
/// as an MPD output after a restart.
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_connect(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().connect(&body.address).await)?;

    // Create (or update) the MPD output config fragment for this BT speaker.
    let devices = s.bluetooth().list_devices().await;
    if let Some(device) = devices.iter().find(|d| d.address == body.address) {
        mpd_integration::create_fragment(
            device,
            s.device_configs(),
            |pending| {
                if pending {
                    s.set_config_restart_pending(true);
                }
            },
        )
        .map_err(|e| AppError::Bluetooth(e.to_string()))?;
        ensure_output_include(&s).await?;
    }

    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/wake-connect` — wake and connect to a paired device
/// that may be in sleep/standby mode.
///
/// Many Bluetooth speakers go into deep sleep after being idle. This endpoint
/// attempts to connect with retry logic and delays to give the device time
/// to wake up and respond.
///
/// Also creates an MPD output config fragment for the speaker so it appears
/// as an MPD output after a restart.
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_wake_connect(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().wake_and_connect(&body.address).await)?;

    // Create (or update) the MPD output config fragment for this BT speaker.
    let devices = s.bluetooth().list_devices().await;
    if let Some(device) = devices.iter().find(|d| d.address == body.address) {
        mpd_integration::create_fragment(
            device,
            s.device_configs(),
            |pending| {
                if pending {
                    s.set_config_restart_pending(true);
                }
            },
        )
        .map_err(|e| AppError::Bluetooth(e.to_string()))?;
        ensure_output_include(&s).await?;
    }

    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/disconnect` — disconnect a device (keeps pairing).
///
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_disconnect(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().disconnect(&body.address).await)?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/forget` — unpair / remove a device.
///
/// Also removes the MPD output config fragment for this device.
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_forget(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    // Remove the MPD config fragment before unpairing.
    let devices = s.bluetooth().list_devices().await;
    if let Some(device) = devices.iter().find(|d| d.address == body.address) {
        let _ = mpd_integration::remove_fragment(
            device,
            s.device_configs(),
            |pending| s.set_config_restart_pending(pending),
        );
    }

    bt_available(s.bluetooth().forget(&body.address).await)?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/remove-output` — explicitly remove the MPD output
/// config fragment for a device (without disconnecting or unpairing).
///
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_remove_output(
    State(s): State<AppState>,
    Json(body): Json<AddressBody>,
) -> AppResult<StatusCode> {
    let devices = s.bluetooth().list_devices().await;
    let device = devices
        .iter()
        .find(|d| d.address == body.address)
        .ok_or_else(|| AppError::NotFound(format!("BT device {}", body.address)))?;

    mpd_integration::remove_fragment(
        device,
        s.device_configs(),
        |pending| s.set_config_restart_pending(pending),
    )
    .map_err(|e| AppError::Bluetooth(e.to_string()))?;

    Ok(StatusCode::OK)
}

// ── BT input (A2DP sink) — stubs filled by U4 ─────────────────────

/// `POST /api/bluetooth/input/enable` — enable A2DP sink input.
async fn bluetooth_input_enable(State(s): State<AppState>) -> AppResult<StatusCode> {
    let input = s.bluetooth().input();
    input.enable().await.map_err(|e| AppError::Bluetooth(e.to_string()))?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/input/disable` — disable A2DP sink input.
async fn bluetooth_input_disable(State(s): State<AppState>) -> AppResult<StatusCode> {
    let input = s.bluetooth().input();
    input.disable().await.map_err(|e| AppError::Bluetooth(e.to_string()))?;
    Ok(StatusCode::OK)
}

/// `GET /api/bluetooth/input/status` — is A2DP sink active?
async fn bluetooth_input_status(State(s): State<AppState>) -> Json<InputStatusResponse> {
    let input = s.bluetooth().input();
    Json(InputStatusResponse {
        enabled: input.is_enabled(),
        streaming: input.is_streaming(),
    })
}

/// `GET /api/bluetooth/devices/audio` — list only audio output devices
/// (speakers, headphones, headsets, etc.).
async fn bluetooth_audio_devices(State(s): State<AppState>) -> AppResult<Json<Vec<BtDevice>>> {
    bt_available(s.bluetooth().check_available().await)?;
    // The linux implementation has list_audio_devices, but the trait doesn't.
    // We'll filter on the frontend for now, or we can add it to the stub.
    // For now, return all devices and let the frontend filter.
    // TODO: Add list_audio_devices to the BluetoothManager trait.
    Ok(Json(s.bluetooth().list_devices().await))
}

/// `POST /api/bluetooth/rename` — set a user-friendly name (alias) for a paired device.
///
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX", "name": "My Speaker" }`.
async fn bluetooth_rename(
    State(s): State<AppState>,
    Json(body): Json<RenameBody>,
) -> AppResult<StatusCode> {
    bt_available(s.bluetooth().set_alias(&body.address, &body.name).await)?;
    Ok(StatusCode::OK)
}

/// `POST /api/bluetooth/test-connect` — test connectivity to a device.
///
/// Attempts to connect and then disconnect to verify the device is reachable.
/// Body: `{ "address": "XX:XX:XX:XX:XX:XX" }`.
async fn bluetooth_test_connect(
    State(s): State<AppState>,
    Json(body): Json<TestConnectBody>,
) -> AppResult<Json<TestConnectResponse>> {
    bt_available(s.bluetooth().check_available().await)?;
    match s.bluetooth().test_connectivity(&body.address).await {
        Ok(()) => Ok(Json(TestConnectResponse {
            success: true,
            message: "Successfully connected and disconnected".to_string(),
        })),
        Err(e) => Ok(Json(TestConnectResponse {
            success: false,
            message: format!("Connection test failed: {}", e),
        })),
    }
}
