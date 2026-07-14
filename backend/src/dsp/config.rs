use crate::dsp::profile::{DspMode, DspProfile, EqBandType, ResamplePreset};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CamillaConfig {
    pub devices: Devices,
    pub samplerate: u32,
    pub channels: u32,
    #[serde(default)]
    pub mixers: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub filters: HashMap<String, serde_yaml::Value>,
    pub pipeline: Vec<PipelineStep>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Devices {
    pub capture: Device,
    pub playback: Device,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Device {
    #[serde(rename = "type")]
    pub dev_type: String,
    pub channels: u32,
    pub device: String,
    pub format: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum PipelineStep {
    Resampler { resampler: Resampler },
    Biquad { channel: u32, filter: BiquadFilter },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Resampler {
    #[serde(rename = "type")]
    pub resampler_type: String,
    pub srate: u32,
    pub quality: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum BiquadFilter {
    Peaking { freq: f64, gain: f64, q: f64 },
    LowShelf { freq: f64, gain: f64, q: f64 },
    HighShelf { freq: f64, gain: f64, q: f64 },
}

fn biquad_filter(band: &crate::dsp::profile::EqBand) -> BiquadFilter {
    match band.band_type {
        EqBandType::Peaking => BiquadFilter::Peaking {
            freq: band.freq,
            gain: band.gain,
            q: band.q,
        },
        EqBandType::LowShelf => BiquadFilter::LowShelf {
            freq: band.freq,
            gain: band.gain,
            q: band.q,
        },
        EqBandType::HighShelf => BiquadFilter::HighShelf {
            freq: band.freq,
            gain: band.gain,
            q: band.q,
        },
    }
}

fn quality_name(preset: ResamplePreset) -> &'static str {
    match preset {
        ResamplePreset::Balanced => "Medium",
        ResamplePreset::High => "High",
        ResamplePreset::Extreme => "VeryHigh",
    }
}

/// Render a CamillaDSP YAML config for the effective profile.
///
/// `capture_rate` is the ALSA loopback sample rate MPD emits at (bit-perfect
/// passthrough keeps this rate; resample mode targets `profile.target_rate`).
pub fn render_camilladsp_config(
    profile: &DspProfile,
    capture_device: &str,
    playback_device: &str,
    capture_rate: u32,
) -> CamillaConfig {
    let profile = profile.effective();
    let channels = 2u32;
    let (samplerate, mut pipeline) = match profile.mode {
        DspMode::BitPerfect => (capture_rate, Vec::new()),
        DspMode::Resample => {
            let target = profile.target_rate.unwrap_or(capture_rate);
            let step = PipelineStep::Resampler {
                resampler: Resampler {
                    resampler_type: "Soxr".to_string(),
                    srate: target,
                    quality: quality_name(profile.preset).to_string(),
                },
            };
            (target, vec![step])
        }
    };

    for band in &profile.eq_bands {
        for ch in 0..channels {
            pipeline.push(PipelineStep::Biquad {
                channel: ch,
                filter: biquad_filter(band),
            });
        }
    }

    CamillaConfig {
        devices: Devices {
            capture: Device {
                dev_type: "Raw".to_string(),
                channels,
                device: capture_device.to_string(),
                format: "S32LE".to_string(),
            },
            playback: Device {
                dev_type: "Raw".to_string(),
                channels,
                device: playback_device.to_string(),
                format: "S32LE".to_string(),
            },
        },
        samplerate,
        channels,
        mixers: HashMap::new(),
        filters: HashMap::new(),
        pipeline,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::profile::{DspMode, EqBand, EqBandType};

    fn base_profile() -> DspProfile {
        DspProfile {
            device: "DAC".into(),
            mode: DspMode::BitPerfect,
            target_rate: None,
            preset: ResamplePreset::default(),
            eq_bands: vec![],
        }
    }

    #[test]
    fn bit_perfect_has_empty_pipeline() {
        let p = base_profile();
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "hw:DAC", 44100);
        assert!(cfg.pipeline.is_empty());
        assert_eq!(cfg.samplerate, 44100);
    }

    #[test]
    fn resample_adds_resampler_step() {
        let mut p = base_profile();
        p.mode = DspMode::Resample;
        p.target_rate = Some(96000);
        p.preset = ResamplePreset::High;
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "hw:DAC", 44100);
        assert_eq!(cfg.samplerate, 96000);
        assert_eq!(cfg.pipeline.len(), 1);
        match &cfg.pipeline[0] {
            PipelineStep::Resampler { resampler } => {
                assert_eq!(resampler.srate, 96000);
                assert_eq!(resampler.quality, "High");
            }
            _ => panic!("expected resampler"),
        }
    }

    #[test]
    fn eq_adds_biquad_per_channel() {
        let mut p = base_profile();
        p.mode = DspMode::Resample;
        p.target_rate = Some(48000);
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "hw:DAC", 44100);
        assert_eq!(cfg.pipeline.len(), 3);
        assert!(matches!(cfg.pipeline[1], PipelineStep::Biquad { channel: 0, .. }));
        assert!(matches!(cfg.pipeline[2], PipelineStep::Biquad { channel: 1, .. }));
    }

    #[test]
    fn bit_perfect_strips_eq_from_effective() {
        let mut p = base_profile();
        p.mode = DspMode::BitPerfect;
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "hw:DAC", 44100);
        assert!(cfg.pipeline.is_empty(), "bit-perfect must not apply EQ");
    }
}
