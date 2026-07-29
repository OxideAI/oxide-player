use crate::config::Config;
use crate::dsp::camilladsp::{DEFAULT_CAPTURE_DEVICE, DEFAULT_CAPTURE_RATE};
use anyhow::Result;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleFormat, StreamConfig};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::broadcast;

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
            bloom_alpha: 0.42,
            bloom_beat: 0.22,
            bloom_energy: 0.28,
            bloom_radius: 1.05,
            bar_idle: 0.18,
            bar_peak: 0.82,
            bar_gap: 3.0,
            bar_radius: 6.0,
            phase_speed: 1.1,
            blur: 6.0,
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
        VizParams {
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
}

impl VisualizerAnalyzer {
    /// Build the analyzer and start capturing if `enabled`. When disabled, the
    /// broadcast channel still exists (so clients can connect and receive an
    /// empty frame) but no audio device is opened.
    pub fn new(config: &Config) -> Self {
        let (tx, _rx) = broadcast::channel(64);
        let analyzer = VisualizerAnalyzer {
            inner: Arc::new(Inner { tx }),
        };
        if config.visualizer_fft {
            if let Err(e) = analyzer.start(config) {
                tracing::warn!("visualizer FFT capture did not start: {e}");
            }
        }
        analyzer
    }

    /// Subscribe a client to spectrum frames.
    pub fn subscribe(&self) -> broadcast::Receiver<SpectrumFrame> {
        self.inner.tx.subscribe()
    }

    fn start(&self, config: &Config) -> Result<()> {
        let device_name = config
            .visualizer_capture_device
            .clone()
            .or_else(|| config.camilladsp_capture_device.clone())
            .unwrap_or_else(|| DEFAULT_CAPTURE_DEVICE.to_string());
        let rate = config
            .visualizer_capture_rate
            .or(config.camilladsp_capture_rate)
            .unwrap_or(DEFAULT_CAPTURE_RATE);

        let host = cpal::default_host();
        let device = pick_device(&host, &device_name)?;
        let config_range = device
            .default_input_config()
            .map_err(|e| anyhow::anyhow!("no input config for {device_name}: {e}"))?;
        let sample_format = config_range.sample_format();
        let channels = config_range.channels().max(1);

        let stream_config = StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size: BufferSize::Default,
        };

        let tx = self.inner.tx.clone();
        // `window` holds the most recent samples; the callback writes into a
        // ring and a publishing task reads it out on the PUBLISH_HZ cadence.
        let window_size = (rate as f64 * WINDOW_SECONDS).round() as usize;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(window_size.next_power_of_two());

        let shared = SharedState::new(window_size, channels as usize);
        let shared_capture = shared.clone();

        let err_tx = tx.clone();
        let stream = match sample_format {
            SampleFormat::F32 => build_stream::<f32>(
                &device,
                &stream_config,
                shared_capture,
                &err_tx,
            )?,
            SampleFormat::I16 => build_stream::<i16>(
                &device,
                &stream_config,
                shared_capture,
                &err_tx,
            )?,
            SampleFormat::U16 => build_stream::<u16>(
                &device,
                &stream_config,
                shared_capture,
                &err_tx,
            )?,
            other => anyhow::bail!("unsupported capture sample format: {other:?}"),
        };

        stream.play().map_err(|e| anyhow::anyhow!("play stream: {e}"))?;
        // Keep the stream alive for the process lifetime without holding a
        // `!Send` `cpal::Stream` inside the `Send` `AppState`. Leaking is safe:
        // capture runs until the process exits, which is exactly what we want.
        let _ = Box::leak(Box::new(stream));

        // Publisher: pulls the latest window, runs the FFT, groups into BANDS,
        // and broadcasts. Runs detached; never touches the audio callback's hot
        // path so capture stays glitch-free.
         let publish_shared = shared.clone();
         let publish_tx = tx.clone();
         let fft_bins = fft.len();
         tokio::spawn(async move {
             tracing::debug!("visualizer publisher task started");
             let mut planner = FftPlanner::<f32>::new();
             let fft = planner.plan_fft_forward(
                 publish_shared.window_size.next_power_of_two(),
             );
             // Reused scratch buffer — avoids a ~16k-element heap alloc every
             // frame (40 fps) on the publisher hot path.
             let padded = publish_shared.window_size.next_power_of_two();
             let mut scratch = vec![Complex::new(0.0f32, 0.0f32); padded];
             let mut interval =
                 tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / PUBLISH_HZ));
             let mut published = 0u32;
             loop {
                 interval.tick().await;
                 let frame = compute_frame(&publish_shared, &fft, fft_bins, &mut scratch);
                match publish_tx.send(frame) {
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

        tracing::info!(
            "visualizer FFT capture started on device '{device_name}' @ {rate} Hz, {channels}ch, {BANDS} bands"
        );
        Ok(())
    }
}

/// Resolve the capture device by exact name, then by substring, then fall back
/// to the platform default input device so the feature works with zero config
/// on macOS (uses the default mic / loopback if one is set as default input).
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
            if d.description().map(|desc| desc.name() == name).unwrap_or(false) {
                return Ok(d.clone());
            }
        }
        for d in &devices {
            if d.description().map(|desc| desc.name().contains(name)).unwrap_or(false) {
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

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<SharedState>,
    err_tx: &broadcast::Sender<SpectrumFrame>,
) -> Result<cpal::Stream>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
    f32: cpal::FromSample<T>,
{
    let err_tx = err_tx.clone();
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
    let mut counts = vec![0usize; BANDS];
    for i in min_bin..=max_bin {
        let t = (i as f64).ln();
        let norm = if log_max > log_min {
            (t - log_min) / (log_max - log_min)
        } else {
            0.0
        };
        let band = (norm * (BANDS as f64 - 1.0)).round() as usize;
        let band = band.min(BANDS - 1);
        // `mag[i]` is already scaled to a 0..1 amplitude; soft-clip to be safe.
        let v = mag[i].clamp(0.0, 1.0);
        bins[band] += v;
        counts[band] += 1;
    }
    for b in 0..BANDS {
        if counts[b] > 0 {
            bins[b] /= counts[b] as f32;
        }
        // Perceptual lift: sqrt so quiet bands remain visible; clamp to 0..1.
        bins[b] = bins[b].sqrt().clamp(0.0, 1.0);
    }

    SpectrumFrame { bins, level }
}
