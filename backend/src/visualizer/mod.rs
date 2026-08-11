use crate::config::Config;
use crate::dsp::camilladsp::{DEFAULT_CAPTURE_DEVICE, DEFAULT_CAPTURE_RATE};
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::broadcast;

/// Runtime lifecycle of the visualizer capture path.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum VisualizerStatusState {
    Disabled,
    EnabledPendingRestart,
    Running,
    WaitingForCapture,
    #[serde(rename = "startup/runtime-error")]
    StartupRuntimeError,
}

/// Observable visualizer configuration and capture lifecycle.
#[derive(Clone, Debug, Serialize)]
pub struct VisualizerStatus {
    pub status: VisualizerStatusState,
    pub configured_enabled: bool,
    pub applied_enabled: bool,
    pub configured_source: Option<String>,
    pub configured_rate: Option<u32>,
    pub applied_source: Option<String>,
    pub applied_rate: Option<u32>,
    pub restart_required: bool,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VisualizerStatusEvent {
    CaptureStarted,
    CaptureWaiting,
    StartupRuntimeError(String),
    RuntimeError(String),
}

fn transition_status(status: &Arc<Mutex<VisualizerStatus>>, event: VisualizerStatusEvent) {
    let mut current = status.lock().expect("visualizer status lock");
    *current = reduce_visualizer_status(current.clone(), event);
}

/// Pure lifecycle reducer used by the analyzer and deterministic unit tests.
pub fn reduce_visualizer_status(
    mut current: VisualizerStatus,
    event: VisualizerStatusEvent,
) -> VisualizerStatus {
    match event {
        VisualizerStatusEvent::CaptureStarted => {
            current.status = VisualizerStatusState::Running;
            current.detail = None;
        }
        VisualizerStatusEvent::CaptureWaiting => {
            current.status = VisualizerStatusState::WaitingForCapture;
            current.detail = None;
        }
        VisualizerStatusEvent::StartupRuntimeError(detail)
        | VisualizerStatusEvent::RuntimeError(detail) => {
            current.status = VisualizerStatusState::StartupRuntimeError;
            current.detail = Some(detail);
        }
    }
    current
}

fn configured_source(config: &Config) -> Option<String> {
    config
        .visualizer_fifo
        .clone()
        .or_else(|| config.visualizer_capture_device.clone())
        .or_else(|| config.camilladsp_capture_device.clone())
        .or_else(|| Some(DEFAULT_CAPTURE_DEVICE.to_string()))
}

fn configured_rate(config: &Config) -> Option<u32> {
    config
        .visualizer_capture_rate
        .or(config.camilladsp_capture_rate)
        .or(Some(DEFAULT_CAPTURE_RATE))
}

fn initial_status(config: &Config) -> VisualizerStatus {
    VisualizerStatus {
        status: if config.visualizer_fft {
            VisualizerStatusState::WaitingForCapture
        } else {
            VisualizerStatusState::Disabled
        },
        configured_enabled: config.visualizer_fft,
        applied_enabled: config.visualizer_fft,
        configured_source: configured_source(config),
        configured_rate: configured_rate(config),
        applied_source: configured_source(config),
        applied_rate: configured_rate(config),
        restart_required: false,
        detail: None,
    }
}

/// Number of magnitude bins published to clients. The spectrum is log-grouped
/// into this many bands so the visualizer gets a musically useful, evenly
/// spaced low→high breakdown rather than raw FFT bins. The frontend mirrors
/// this value in `Visualizer.tsx` (BARS) — keep the two in sync, since the
/// WebSocket frames carry exactly BANDS floats.
pub const BANDS: usize = 72;

/// How often (Hz) the analyzer publishes a frame. ~40 fps is smooth without
/// flooding the websocket or starving the capture thread.
const PUBLISH_HZ: f64 = 40.0;

/// Frames of samples buffered into one FFT window. At 44.1 kHz a quarter-second
/// window gives ~3 Hz frequency resolution — plenty for a visualizer.
const WINDOW_SECONDS: f64 = 0.25;

/// Live FFT magnitude frame. `bins` are normalized 0..1 (peak ≈ 1) and ordered
/// low frequency → high frequency. `level` is the overall RMS energy 0..1.
#[derive(Clone, Debug, Serialize)]
pub struct SpectrumFrame {
    pub bins: Vec<f32>,
    pub level: f32,
}

/// Tunable look-and-feel parameters for the Kiosk visualizer. Persisted to
/// `<data_dir>/vizparams.json` so a tuned look survives restarts (loaded from
/// disk, not the browser). Mirrors the frontend `VizParams` shape.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VizParams {
    #[serde(default)]
    pub style: String,
    pub bloom_alpha: f32,
    pub bloom_beat: f32,
    pub bloom_energy: f32,
    pub bloom_radius: f32,
    pub bar_idle: f32,
    pub bar_peak: f32,
    pub bar_gap: f32,
    pub bar_radius: f32,
    pub phase_speed: f32,
    pub blur: f32,
}

impl Default for VizParams {
    fn default() -> Self {
        VizParams {
            style: "bars".to_string(),
            bloom_alpha: 0.28,
            bloom_beat: 0.16,
            bloom_energy: 0.5,
            bloom_radius: 0.92,
            bar_idle: 0.08,
            bar_peak: 0.92,
            bar_gap: 3.0,
            bar_radius: 6.0,
            phase_speed: 1.1,
            blur: 3.0,
        }
    }
}

impl VizParams {
    /// Load params from `<data_dir>/vizparams.json`, falling back to defaults
    /// when the file is missing or unparseable. Best-effort: a missing/bad file
    /// is never fatal — the visualizer just uses the code defaults.
    pub fn load(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("vizparams.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map(|p: VizParams| p.clamp())
                .unwrap_or_else(|e| {
                    tracing::warn!("visualizer params unparseable, using defaults: {e}");
                    VizParams::default()
                }),
            Err(_) => VizParams::default(),
        }
    }

    /// Clamp every field to a sane range so a client-supplied (or hand-edited)
    /// `vizparams.json` can never produce a degenerate visual or a panic in the
    /// renderer. Mirrors `CoverOptimization::from_config` in config.rs.
    fn clamp(self) -> Self {
        let style = match self.style.as_str() {
            "mirrored" | "circular" | "waveform" | "ring" => self.style,
            _ => "bars".to_string(),
        };
        VizParams {
            style,
            bloom_alpha: self.bloom_alpha.clamp(0.0, 1.0),
            bloom_beat: self.bloom_beat.clamp(0.0, 1.0),
            bloom_energy: self.bloom_energy.clamp(0.0, 1.0),
            bloom_radius: self.bloom_radius.clamp(0.0, 4.0),
            bar_idle: self.bar_idle.clamp(0.0, 1.0),
            bar_peak: self.bar_peak.clamp(0.0, 1.0),
            bar_gap: self.bar_gap.clamp(0.0, 32.0),
            bar_radius: self.bar_radius.clamp(0.0, 32.0),
            phase_speed: self.phase_speed.clamp(0.0, 16.0),
            blur: self.blur.clamp(0.0, 64.0),
        }
    }

    /// Persist params to `<data_dir>/vizparams.json` (atomic temp+rename).
    pub fn save(&self, data_dir: &std::path::Path) -> Result<()> {
        let params = self.clone().clamp();
        std::fs::create_dir_all(data_dir)
            .map_err(|e| anyhow::anyhow!("create data dir: {e}"))?;
        let path = data_dir.join("vizparams.json");
        let text = serde_json::to_string_pretty(&params)
            .map_err(|e| anyhow::anyhow!("serialize viz params: {e}"))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| anyhow::anyhow!("write viz params: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| anyhow::anyhow!("rename viz params: {e}"))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct VisualizerAnalyzer {
    inner: Arc<Inner>,
}

struct Inner {
    tx: broadcast::Sender<SpectrumFrame>,
    status: Arc<Mutex<VisualizerStatus>>,
}

impl VisualizerAnalyzer {
    fn transition(&self, event: VisualizerStatusEvent) {
        transition_status(&self.inner.status, event);
    }

    /// Return runtime state joined with the currently configured values.
    /// Config changes are persisted before a process restart, so a mismatch
    /// between configured and applied values is explicitly pending.
    pub fn status(&self, config: &Config) -> VisualizerStatus {
        let mut status = self.inner.status.lock().expect("visualizer status lock").clone();
        status.configured_enabled = config.visualizer_fft;
        status.configured_source = configured_source(config);
        status.configured_rate = configured_rate(config);
        status.restart_required = status.configured_enabled != status.applied_enabled
            || status.configured_source != status.applied_source
            || status.configured_rate != status.applied_rate;
        if status.restart_required {
            status.status = VisualizerStatusState::EnabledPendingRestart;
            status.detail = None;
        } else if !status.applied_enabled {
            status.status = VisualizerStatusState::Disabled;
        }
        status
    }
}


impl VisualizerAnalyzer {
    /// Build the analyzer and start capturing if `enabled`. When disabled, the
    /// broadcast channel still exists (so clients can connect and receive an
    /// empty frame) but no audio device is opened.
    pub fn new(config: &Config) -> Self {
        let (tx, _rx) = broadcast::channel(64);
        let analyzer = VisualizerAnalyzer {
            inner: Arc::new(Inner {
                tx,
                status: Arc::new(Mutex::new(initial_status(config))),
            }),
        };
        if config.visualizer_fft {
            if let Err(e) = analyzer.start(config) {
                tracing::warn!("visualizer FFT capture did not start: {e}");
                analyzer.transition(VisualizerStatusEvent::StartupRuntimeError(e.to_string()));
            }
        }
        analyzer
    }


    /// Subscribe a client to spectrum frames.
    pub fn subscribe(&self) -> broadcast::Receiver<SpectrumFrame> {
        self.inner.tx.subscribe()
    }

    fn start(&self, config: &Config) -> Result<()> {
        // MPD fifo tap takes precedence over ALSA capture when configured: it
        // works in every output mode and avoids loopback substream contention
        // with CamillaDSP (snd-aloop delivers a substream to one capture
        // client only).
        if let Some(fifo) = config.visualizer_fifo.as_deref() {
            return self.start_fifo(fifo);
        }
        let device_name = config
            .visualizer_capture_device
            .clone()
            .or_else(|| config.camilladsp_capture_device.clone())
            .unwrap_or_else(|| DEFAULT_CAPTURE_DEVICE.to_string());
        let host = cpal::default_host();
        let device = pick_device(&host, &device_name)?;
        let default_config = device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("no input config for {device_name}: {e}"))?;
        let requested_rate = config
            .visualizer_capture_rate
            .or(config.camilladsp_capture_rate)
            .unwrap_or(DEFAULT_CAPTURE_RATE);
        // A stale fixed rate is enough to make CPAL reject the stream. Keep
        // the requested rate when the selected device supports it, otherwise
        // use that device's known-good default input configuration.
        let selected_config = device
            .supported_input_configs()
            .ok()
            .and_then(|ranges| {
                let ranges: Vec<_> = ranges.collect();
                // 1. The device's default format at the requested rate.
                if let Some(config) = ranges
                    .iter()
                    .filter(|range| {
                        range.channels() == default_config.channels()
                            && range.sample_format() == default_config.sample_format()
                    })
                    .find_map(|range| range.try_with_sample_rate(requested_rate))
                {
                    return Some(config);
                }
                // 2. Any buildable format at the requested rate — a device
                //    whose default format is unsupported at that rate must not
                //    kill capture.
                ranges
                    .iter()
                    .filter(|range| {
                        range.channels() == default_config.channels()
                            && has_stream_builder(range.sample_format())
                    })
                    .find_map(|range| range.try_with_sample_rate(requested_rate))
            })
            .unwrap_or_else(|| {
                if requested_rate != default_config.sample_rate() {
                    tracing::warn!(
                        "visualizer device '{device_name}' does not support {requested_rate} Hz; using {} Hz",
                        default_config.sample_rate()
                    );
                }
                default_config
            });
        let sample_format = selected_config.sample_format();
        let channels = selected_config.channels().max(1);
        let rate = selected_config.sample_rate();
        let stream_config: StreamConfig = selected_config.into();

        let tx = self.inner.tx.clone();
        // `window` holds the most recent samples; the callback writes into a
        // ring and a publishing task reads it out on the PUBLISH_HZ cadence.
        let window_size = (rate as f64 * WINDOW_SECONDS).round() as usize;

        let shared = SharedState::new(window_size, channels as usize);
        let shared_capture = shared.clone();

        let err_tx = tx.clone();
        let status = self.inner.status.clone();
        let stream = build_stream_for_format(
            &device,
            &stream_config,
            sample_format,
            shared_capture,
            &err_tx,
            status,
        )?;

        stream.play().map_err(|e| anyhow::anyhow!("play stream: {e}"))?;
        // Keep the stream alive for the process lifetime without holding a
        // `!Send` `cpal::Stream` inside the `Send` `AppState`. Leaking is safe:
        // capture runs until the process exits, which is exactly what we want.
        self.spawn_publisher(shared, tx);
        self.transition(VisualizerStatusEvent::CaptureStarted);
        tracing::info!(
            "visualizer FFT capture started on device '{device_name}' @ {rate} Hz, {channels}ch, {BANDS} bands"
        );
        Ok(())
    }

    /// Spawn the FFT publisher: pulls the latest window, runs the FFT, groups
    /// into BANDS, and broadcasts at PUBLISH_HZ. Runs detached and never
    /// touches the capture hot path, so capture stays glitch-free.
    fn spawn_publisher(&self, shared: Arc<SharedState>, tx: broadcast::Sender<SpectrumFrame>) {
        tokio::spawn(async move {
            tracing::debug!("visualizer publisher task started");
            let padded = shared.window_size.next_power_of_two();
            let mut planner = FftPlanner::<f32>::new();
            let fft = planner.plan_fft_forward(padded);
            // Reused scratch buffer — avoids a ~16k-element heap alloc every
            // frame (40 fps) on the publisher hot path.
            let mut scratch = vec![Complex::new(0.0f32, 0.0f32); padded];
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / PUBLISH_HZ));
            let mut published = 0u32;
            loop {
                interval.tick().await;
                let frame = compute_frame(&shared, &fft, padded, &mut scratch);
                match tx.send(frame) {
                    Ok(n) => {
                        published += 1;
                        if published % 200 == 0 {
                            tracing::debug!("visualizer published {published} frames, receivers={n}");
                        }
                    }
                    Err(_) => {
                        tracing::debug!("visualizer no receivers, frame dropped");
                    }
                }
            }
        });
    }

    /// Tap the MPD `fifo` output (format "44100:16:2", S16_LE interleaved
    /// stereo) instead of an ALSA capture device. MPD feeds the fifo in every
    /// output mode — Bluetooth, DSP loopback, analog — so the visualizer
    /// animates regardless of the active routing, with no substream contention
    /// with CamillaDSP. MPD creates the fifo file at startup and may not exist
    /// yet (the backend autostarts MPD after the analyzer), so the reader
    /// retries until it appears and reopens after MPD restarts.
    fn start_fifo(&self, path: &str) -> Result<()> {
        const RATE: u32 = 44100;
        const CHANNELS: usize = 2;
        let window_size = (RATE as f64 * WINDOW_SECONDS).round() as usize;
        let shared = SharedState::new(window_size, CHANNELS);
        let shared_reader = shared.clone();
        let tx = self.inner.tx.clone();
        let path_owned = path.to_string();
        let status = self.inner.status.clone();
        let status_reader = status.clone();
        std::thread::Builder::new()
            .name("visualizer-fifo".into())
            .spawn(move || {
                loop {
                    match std::fs::File::open(&path_owned) {
                        Ok(file) => {
                            tracing::info!("visualizer fifo '{path_owned}' open, reading S16_LE stereo");
                            transition_status(&status_reader, VisualizerStatusEvent::CaptureStarted);
                            match read_fifo(file, &shared_reader, CHANNELS) {
                                Ok(()) => {
                                    transition_status(&status_reader, VisualizerStatusEvent::CaptureWaiting);
                                }
                                Err(e) => {
                                    tracing::warn!("visualizer fifo '{path_owned}' read error: {e}");
                                    transition_status(
                                        &status_reader,
                                        VisualizerStatusEvent::RuntimeError(e.to_string()),
                                    );
                                }
                            }
                            // Writer closed (MPD restart/stop): reopen shortly.
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                            tracing::debug!("visualizer fifo '{path_owned}' not present yet, retrying");
                            transition_status(&status_reader, VisualizerStatusEvent::CaptureWaiting);
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                        Err(e) => {
                            tracing::warn!("visualizer fifo '{path_owned}' cannot be opened: {e}");
                            transition_status(
                                &status_reader,
                                VisualizerStatusEvent::RuntimeError(e.to_string()),
                            );
                            std::thread::sleep(std::time::Duration::from_secs(2));
                        }
                    }
                }
            })
            .map_err(|e| anyhow::anyhow!("spawn fifo reader thread: {e}"))?;

        self.spawn_publisher(shared, tx);
        self.transition(VisualizerStatusEvent::CaptureWaiting);
        tracing::info!(
            "visualizer FFT capture started on MPD fifo '{path}' @ {RATE} Hz, {CHANNELS}ch, {BANDS} bands"
        );
        Ok(())
    }
}

/// Read S16_LE interleaved PCM from the MPD fifo until the writer closes.
/// Returns `Ok` on a normal writer close (MPD restarted); hard I/O errors are
/// surfaced to the caller for logging and reopen.
fn read_fifo(
    mut file: std::fs::File,
    shared: &SharedState,
    channels: usize,
) -> std::io::Result<()> {
    use std::io::Read;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(()); // writer closed
        }
        push_i16_pcm(shared, &buf[..n], channels);
    }
}

/// Decode interleaved S16_LE frames, mix down to mono into the shared window
/// (oldest samples dropped past `window_size`). Partial trailing frames are
/// skipped, so misaligned reads cannot corrupt the stream.
fn push_i16_pcm(shared: &SharedState, data: &[u8], channels: usize) {
    let mut guard = shared.samples.lock().unwrap();
    let ch = channels.max(1) as f32;
    for frame in data.chunks_exact(channels * 2) {
        let mut sum = 0.0f32;
        for sample in frame.chunks_exact(2).take(channels) {
            let raw = i16::from_le_bytes([sample[0], sample[1]]);
            sum += raw as f32 / 32768.0;
        }
        guard.push(sum / ch);
    }
    if guard.len() > shared.window_size {
        let drop = guard.len() - shared.window_size;
        guard.drain(0..drop);
    }
}
/// Match a configured capture device against CPAL's display name or stable ID.
/// ALSA commonly exposes `hw:Loopback,1` as an ID while its display name is
/// only `Loopback`.
fn device_text_matches(requested: &str, candidate: &str) -> bool {
    !requested.is_empty() && (candidate == requested || candidate.contains(requested))
}

fn pick_device(
    host: &cpal::Host,
    name: &str,
) -> Result<cpal::Device> {
    // An empty name can't be an intentional match — skip straight to the
    // platform default rather than letting `contains("")` grab the first
    // enumerated device.
    if name.is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("no default input device available"));
    }
    if let Ok(devices) = host.input_devices().map(|d| d.collect::<Vec<_>>()) {
        for d in &devices {
            if d.id()
                .map(|id| device_text_matches(name, id.id()))
                .unwrap_or(false)
            {
                return Ok(d.clone());
            }
        }
        for d in &devices {
            if d.description()
                .map(|desc| device_text_matches(name, desc.name()))
                .unwrap_or(false)
            {
                return Ok(d.clone());
            }
        }
    }
    host.default_input_device()
        .ok_or_else(|| anyhow::anyhow!("no default input device available"))
}

/// Double-buffered shared state between the capture callback and the publisher.
/// The callback appends mono-mixed samples to `samples`; the publisher reads
/// `samples` and treats it as a window (oldest-first) when computing the FFT.
struct SharedState {
    window_size: usize,
    channels: usize,
    // `std::sync::Mutex` (not tokio's): the capture callback runs on a dedicated
    // OS audio thread, not the async runtime, so it must never call
    // `blocking_lock`. A plain OS mutex is correct and panic-free here.
    samples: Mutex<Vec<f32>>,
}

impl SharedState {
    fn new(window_size: usize, channels: usize) -> Arc<Self> {
        Arc::new(Self {
            window_size,
            channels,
            samples: Mutex::new(Vec::with_capacity(window_size)),
        })
    }
}

/// Formats the analyzer can build capture streams for, ordered by preference
/// (lossless-to-f32 first). snd-aloop defaults its capture format to S32_LE —
/// cpal `I32` — so integer formats must be listed or loopback capture never
/// starts. Keep in sync with `build_stream_for_format`.
const BUILDER_FORMATS: [SampleFormat; 12] = [
    SampleFormat::F32,
    SampleFormat::I16,
    SampleFormat::U16,
    SampleFormat::I32,
    SampleFormat::I24,
    SampleFormat::U24,
    SampleFormat::I8,
    SampleFormat::U8,
    SampleFormat::I64,
    SampleFormat::U64,
    SampleFormat::F64,
    SampleFormat::U32,
];

fn has_stream_builder(format: SampleFormat) -> bool {
    BUILDER_FORMATS.contains(&format)
}

/// Dispatch a device's sample format to the matching `build_stream::<T>`.
/// Exhaustive over every `cpal::SampleFormat` variant: a device whose default
/// capture format lacks an arm here (e.g. snd-aloop's S32_LE → `I32`) makes
/// the whole visualizer fail to start with "unsupported capture sample format".
fn build_stream_for_format(
    device: &cpal::Device,
    config: &StreamConfig,
    sample_format: SampleFormat,
    shared: Arc<SharedState>,
    err_tx: &broadcast::Sender<SpectrumFrame>,
    status: Arc<Mutex<VisualizerStatus>>,
) -> Result<cpal::Stream> {
    match sample_format {
        SampleFormat::I8 => build_stream::<i8>(device, config, shared, err_tx, status),
        SampleFormat::I16 => build_stream::<i16>(device, config, shared, err_tx, status),
        SampleFormat::I24 => build_stream::<cpal::I24>(device, config, shared, err_tx, status),
        SampleFormat::I32 => build_stream::<i32>(device, config, shared, err_tx, status),
        SampleFormat::I64 => build_stream::<i64>(device, config, shared, err_tx, status),
        SampleFormat::U8 => build_stream::<u8>(device, config, shared, err_tx, status),
        SampleFormat::U16 => build_stream::<u16>(device, config, shared, err_tx, status),
        SampleFormat::U24 => build_stream::<cpal::U24>(device, config, shared, err_tx, status),
        SampleFormat::U32 => build_stream::<u32>(device, config, shared, err_tx, status),
        SampleFormat::U64 => build_stream::<u64>(device, config, shared, err_tx, status),
        SampleFormat::F32 => build_stream::<f32>(device, config, shared, err_tx, status),
        SampleFormat::F64 => build_stream::<f64>(device, config, shared, err_tx, status),
        // `SampleFormat` is #[non_exhaustive]: a format cpal adds later has no
        // builder here, and `every_cpal_sample_format_has_a_stream_builder`
        // must be extended to cover it.
        other => anyhow::bail!("unsupported capture sample format: {other:?}"),
    }
}
fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<SharedState>,
    err_tx: &broadcast::Sender<SpectrumFrame>,
    status: Arc<Mutex<VisualizerStatus>>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
    f32: cpal::FromSample<T>,
{
    let err_tx = err_tx.clone();
    let status = status.clone();
    let stream = device
        .build_input_stream(
            config.clone(),
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut guard = shared.samples.lock().unwrap();
                // Mix down to mono by averaging channels, then append. Keep only
                // the most recent `window_size` samples as a sliding window.
                let ch = shared.channels.max(1) as f32;
                for frame in data.chunks(shared.channels) {
                    let mono: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum::<f32>() / ch;
                    guard.push(mono);
                }
                if guard.len() > shared.window_size {
                    let drop = guard.len() - shared.window_size;
                    guard.drain(0..drop);
                }
            },
            move |err| {
                tracing::warn!("visualizer capture error: {err}");
                transition_status(
                    &status,
                    VisualizerStatusEvent::RuntimeError(err.to_string()),
                );
                let _ = err_tx;
            },
            None,
        )
        .map_err(|e| anyhow::anyhow!("build input stream: {e}"))?;
    Ok(stream)
}

/// Run the FFT over the current window and group into `BANDS` log-spaced bins.
/// `scratch` is a caller-owned, reusable `padded`-length buffer (avoids a
/// per-frame allocation).
fn compute_frame(
    shared: &SharedState,
    fft: &Arc<dyn rustfft::Fft<f32>>,
    fft_bins: usize,
    scratch: &mut [Complex<f32>],
) -> SpectrumFrame {
    let guard = shared.samples.lock().unwrap();
    let n = shared.window_size;
    // Overall RMS energy straight from the time-domain window — independent of
    // the FFT and of player volume; it's the raw PCM amplitude the mic/loopback
    // captured. Used for the halo intensity and idle/signal distinction. Divide
    // by the actual sample count so the level is correct while the buffer is
    // still filling (before it reaches `window_size`).
    let len = guard.len().max(1);
    let sumsq_td: f32 = guard.iter().map(|s| s * s).sum();
    let level = (sumsq_td / len as f32).sqrt().clamp(0.0, 1.0);

    let padded = n.next_power_of_two();
    // Hann window for cleaner spectral leakage, then zero-pad to the FFT size.
    // Reuse `scratch` (already sized `padded`); clear only what we touch.
    for (i, s) in guard.iter().enumerate() {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n as f32 - 1.0)).cos();
        scratch[i] = Complex::new(s * w, 0.0);
    }
    for i in guard.len()..padded {
        scratch[i] = Complex::new(0.0, 0.0);
    }
    fft.process(scratch);

    // Magnitude of the first half (real signal → symmetric spectrum). Each bin's
    // magnitude is ~amplitude * N/2, so dividing by N/2 recovers the per-bin
    // amplitude in 0..1 (full-scale float), independent of window size.
    let half = padded / 2;
    let mut mag = vec![0.0f32; half];
    let scale = (n as f32 / 2.0).max(1.0);
    for i in 0..half {
        let m = (scratch[i].re * scratch[i].re + scratch[i].im * scratch[i].im).sqrt() / scale;
        mag[i] = m;
    }

    // Log-spaced grouping into BANDS bins. Frequency bin i corresponds to
    // i * (rate/2) / half Hz; we map band edges logarithmically so bass gets
    // fair representation instead of being crushed into the first few bins.
    let mut bins = vec![0.0f32; BANDS];
    let min_bin = 1usize; // skip DC
    let max_bin = half.min(fft_bins / 2).saturating_sub(1).max(min_bin);
    let log_min = (min_bin as f64).ln();
    let log_max = (max_bin as f64).ln();
    for i in min_bin..=max_bin {
        let t = (i as f64).ln();
        let norm = if log_max > log_min {
            (t - log_min) / (log_max - log_min)
        } else {
            0.0
        };
        let band = (norm * (BANDS as f64 - 1.0)).round() as usize;
        let band = band.min(BANDS - 1);
        // Keep the strongest FFT bin in each musical band. Averaging dilutes
        // narrow tonal peaks across hundreds of mostly-empty high-frequency
        // bins, making real music appear motionless.
        bins[band] = bins[band].max(mag[i].clamp(0.0, 1.0));
    }
    for value in &mut bins {
        // Perceptual lift: sqrt so quiet bands remain visible; clamp to 0..1.
        *value = value.sqrt().clamp(0.0, 1.0);
    }

    SpectrumFrame { bins, level }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_params_json() -> serde_json::Value {
        serde_json::json!({
            "bloom_alpha": 0.42,
            "bloom_beat": 0.22,
            "bloom_energy": 0.28,
            "bloom_radius": 1.05,
            "bar_idle": 0.18,
            "bar_peak": 0.82,
            "bar_gap": 3.0,
            "bar_radius": 6.0,
            "phase_speed": 1.1,
            "blur": 6.0
        })
    }

    #[test]
    fn legacy_params_default_to_classic_bars() {
        let parsed: VizParams = serde_json::from_value(legacy_params_json()).unwrap();
        assert_eq!(parsed.clamp().style, "bars");
    }

    #[test]
    fn unknown_style_is_normalized_to_classic_bars() {
        let mut json = legacy_params_json();
        json["style"] = serde_json::json!("not-a-style");
        let parsed: VizParams = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.clamp().style, "bars");
    }

    #[test]
    fn fft_frame_reports_signal_energy() {
        let shared = SharedState::new(2048, 1);
        {
            let mut samples = shared.samples.lock().unwrap();
            for index in 0..2048 {
                samples.push((index as f32 * 2.0 * std::f32::consts::PI * 440.0 / 44_100.0).sin() * 0.8);
            }
        }
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(2048);
        let mut scratch = vec![Complex::new(0.0, 0.0); 2048];
        let frame = compute_frame(&shared, &fft, 2048, &mut scratch);
        assert!(frame.level > 0.2);
        assert!(frame.bins.iter().any(|value| *value > 0.1));
        assert!(frame.bins.iter().copied().fold(0.0, f32::max) > 0.5);
    }

    #[test]
    fn fft_frame_keeps_high_frequency_peaks_visible() {
        let shared = SharedState::new(4096, 1);
        {
            let mut samples = shared.samples.lock().unwrap();
            for index in 0..4096 {
                samples.push((index as f32 * 2.0 * std::f32::consts::PI * 12_000.0 / 44_100.0).sin() * 0.8);
            }
        }
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(4096);
        let mut scratch = vec![Complex::new(0.0, 0.0); 4096];
        let frame = compute_frame(&shared, &fft, 4096, &mut scratch);
        assert!(frame.bins.iter().copied().fold(0.0, f32::max) > 0.5);
    }

    #[test]
    fn fifo_pcm_mixes_to_mono_and_fft_reacts() {
        // Regression: the deployed player's visualizer could not capture —
        // snd-aloop delivers a loopback substream to only one capture client
        // (CamillaDSP holds hw:Loopback,1), and Bluetooth mode never writes
        // the loopback at all. The MPD fifo tap decodes S16_LE stereo into the
        // same window the ALSA path uses; this guards the decode + FFT chain.
        let shared = SharedState::new(4096, 2);
        let mut pcm = Vec::new();
        for index in 0..4096 {
            let v = (index as f32 * 2.0 * std::f32::consts::PI * 440.0 / 44_100.0).sin()
                * 0.8
                * 32767.0;
            // Right channel at half amplitude: mono mix must not cancel.
            for raw in [v as i16, (v * 0.5) as i16] {
                pcm.extend_from_slice(&raw.to_le_bytes());
            }
        }
        // A misaligned first read must not corrupt decoding: the 3-byte prefix
        // carries no complete frame (dropped), the remainder decodes 4095
        // frames plus one trailing partial byte that is also dropped.
        push_i16_pcm(&shared, &pcm[..3], 2);
        push_i16_pcm(&shared, &pcm[3..], 2);
        {
            let samples = shared.samples.lock().unwrap();
            assert_eq!(samples.len(), 4095);
        }
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(4096);
        let mut scratch = vec![Complex::new(0.0, 0.0); 4096];
        let frame = compute_frame(&shared, &fft, 4096, &mut scratch);
        assert!(frame.level > 0.2);
        assert!(frame.bins.iter().any(|value| *value > 0.1));
    }

    #[test]
    fn fifo_window_trims_oldest_samples() {
        let shared = SharedState::new(128, 2);
        let mut pcm = Vec::new();
        for _ in 0..512 {
            pcm.extend_from_slice(&1i16.to_le_bytes());
            pcm.extend_from_slice(&(-1i16).to_le_bytes());
        }
        push_i16_pcm(&shared, &pcm, 2);
        let samples = shared.samples.lock().unwrap();
        assert_eq!(samples.len(), 128);
        // Mono mix of +1/-1 is 0; all samples must decode as silence.
        assert!(samples.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn every_cpal_sample_format_has_a_stream_builder() {
        // Regression: snd-aloop exposes S32_LE as its default capture format
        // (cpal `I32`); the old dispatch only handled F32/I16/U16, so capture
        // bailed with "unsupported capture sample format: I32" and the
        // visualizer never started on installed servers. Every SampleFormat
        // variant must dispatch to a builder.
        for format in [
            SampleFormat::I8,
            SampleFormat::I16,
            SampleFormat::I24,
            SampleFormat::I32,
            SampleFormat::I64,
            SampleFormat::U8,
            SampleFormat::U16,
            SampleFormat::U24,
            SampleFormat::U32,
            SampleFormat::U64,
            SampleFormat::F32,
            SampleFormat::F64,
        ] {
            assert!(
                has_stream_builder(format),
                "missing stream builder for {format:?}"
            );
        }
    }

    #[test]
    fn configured_alsa_device_matches_its_cpal_id() {
        assert!(device_text_matches("hw:Loopback,1", "hw:Loopback,1"));
        assert!(device_text_matches("Loopback", "hw:Loopback,1"));
        assert!(!device_text_matches("hw:Loopback,0", "hw:Loopback,1"));
    }
    fn base_status() -> VisualizerStatus {
        VisualizerStatus {
            status: VisualizerStatusState::WaitingForCapture,
            configured_enabled: true,
            applied_enabled: true,
            configured_source: Some("fifo".into()),
            configured_rate: Some(44_100),
            applied_source: Some("fifo".into()),
            applied_rate: Some(44_100),
            restart_required: false,
            detail: None,
        }
    }

    #[test]
    fn visualizer_status_reducer_distinguishes_lifecycle_states() {
        let disabled = VisualizerStatus {
            status: VisualizerStatusState::Disabled,
            configured_enabled: false,
            applied_enabled: false,
            configured_source: None,
            configured_rate: None,
            applied_source: None,
            applied_rate: None,
            restart_required: false,
            detail: None,
        };
        assert_eq!(disabled.status, VisualizerStatusState::Disabled);
        assert_eq!(
            reduce_visualizer_status(base_status(), VisualizerStatusEvent::CaptureWaiting).status,
            VisualizerStatusState::WaitingForCapture
        );
        assert_eq!(
            reduce_visualizer_status(base_status(), VisualizerStatusEvent::CaptureStarted).status,
            VisualizerStatusState::Running
        );
        let error = reduce_visualizer_status(
            base_status(),
            VisualizerStatusEvent::StartupRuntimeError("invalid source".into()),
        );
        assert_eq!(error.status, VisualizerStatusState::StartupRuntimeError);
        assert_eq!(error.detail.as_deref(), Some("invalid source"));
        let waiting = reduce_visualizer_status(error, VisualizerStatusEvent::CaptureWaiting);
        assert_eq!(waiting.status, VisualizerStatusState::WaitingForCapture);
        let reopened = reduce_visualizer_status(waiting, VisualizerStatusEvent::CaptureStarted);
        assert_eq!(reopened.status, VisualizerStatusState::Running);
    }
}
