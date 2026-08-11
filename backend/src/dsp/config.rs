use crate::dsp::profile::{DspMode, DspProfile, EqBandType, ResamplePreset};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CamillaDSP v4.1.3 config structure.
///
/// Key differences from earlier versions:
/// - `samplerate`, `chunksize`, `queuelimit`, `capture_samplerate`, `resampler`
///   are all inside the `devices` block (not at the top level).
/// - Device types use `Alsa`, format uses `S16_LE`.
/// - Pipeline steps are `Filter` (named references) or `Mixer`, not inline
///   Resampler/Biquad steps.
/// - Resampling is configured via `devices.resampler` and `devices.capture_samplerate`
///   rather than a pipeline filter step.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CamillaConfig {
    pub devices: Devices,
    #[serde(default)]
    pub mixers: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    pub filters: HashMap<String, FilterDef>,
    #[serde(default)]
    pub pipeline: Vec<PipelineStep>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Devices {
    pub samplerate: u32,
    pub chunksize: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub queuelimit: Option<u32>,
    /// Capture samplerate — set when resampling is active so CamillaDSP knows
    /// the incoming rate differs from the target `samplerate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture_samplerate: Option<u32>,
    /// Resampler config — present only in Resample mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resampler: Option<ResamplerConfig>,
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

/// v4 AsyncSinc resampler configuration.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResamplerConfig {
    #[serde(rename = "type")]
    pub resampler_type: String,
    pub profile: String,
}

/// A named filter definition referenced from pipeline steps.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FilterDef {
    #[serde(rename = "type")]
    pub filter_type: String,
    pub parameters: FilterParameters,
}

/// A filter's v4 parameters.  The untagged representation preserves the
/// CamillaDSP YAML shape (`type`/biquad fields for Biquads, plain gain fields
/// for a Gain filter).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum FilterParameters {
    Biquad {
        #[serde(rename = "type")]
        param_type: String,
        freq: f64,
        gain: f64,
        q: f64,
    },
    Gain {
        gain: f64,
        inverted: bool,
        mute: bool,
    },
}

/// Pipeline step — v4.1.3 uses `Filter` (named refs) and `Mixer` (named ref).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum PipelineStep {
    Filter { channels: Vec<u32>, names: Vec<String> },
    Mixer { name: String },
}

/// Render a CamillaDSP v4.1.3 YAML config for the effective profile.
///
/// `capture_rate` is the ALSA loopback sample rate MPD emits at.
///
/// - **BitPerfect** mode: same rate capture→playback, no resampler.
///   EQ bands are rendered as named `Biquad` filters in the `filters` section
///   and applied via `Filter` pipeline steps.
/// - **Resample** mode: adds a `resampler` block under `devices` and sets
///   `capture_samplerate` so CamillaDSP knows the incoming rate.  EQ bands
///   are appended after the resampler as filter steps.
pub fn render_camilladsp_config(
    profile: &DspProfile,
    capture_device: &str,
    playback_device: &str,
    capture_rate: u32,
) -> CamillaConfig {
    let profile = profile.effective();
    let channels = 2u32;

    // --- devices-level fields ---
    let (samplerate, capture_samplerate, resampler) = match profile.mode {
        DspMode::BitPerfect => (capture_rate, None, None),
        DspMode::Resample => {
            let target = profile.target_rate.unwrap_or(capture_rate);
            let cfg = ResamplerConfig {
                resampler_type: "AsyncSinc".to_string(),
                profile: resample_profile_name(profile.preset).to_string(),
            };
            (target, Some(capture_rate), Some(cfg))
        }
    };

    // --- filters & pipeline ---
    let mut filters: HashMap<String, FilterDef> = HashMap::new();
    let mut pipeline: Vec<PipelineStep> = Vec::new();

    if profile.preamp != 0.0 {
        filters.insert(
            "preamp".to_string(),
            FilterDef {
                filter_type: "Gain".to_string(),
                parameters: FilterParameters::Gain {
                    gain: profile.preamp,
                    inverted: false,
                    mute: false,
                },
            },
        );
        pipeline.push(PipelineStep::Filter {
            channels: (0..channels).collect(),
            names: vec!["preamp".to_string()],
        });
    }

    for (band_idx, band) in profile.eq_bands.iter().enumerate() {
        let (biquad_type, freq, gain, q) = match band.band_type {
            EqBandType::Peaking => ("Peaking", band.freq, band.gain, band.q),
            EqBandType::LowShelf => ("Lowshelf", band.freq, band.gain, band.q),
            EqBandType::HighShelf => ("Highshelf", band.freq, band.gain, band.q),
        };

        // Create one filter per band (applied to both channels).
        let name = format!("eq_band_{band_idx}");
        filters.insert(
            name.clone(),
            FilterDef {
                filter_type: "Biquad".to_string(),
                parameters: FilterParameters::Biquad {
                    param_type: biquad_type.to_string(),
                    freq,
                    gain,
                    q,
                },
            },
        );
        pipeline.push(PipelineStep::Filter {
            channels: (0..channels).collect(),
            names: vec![name],
        });
    }

    CamillaConfig {
        devices: Devices {
            samplerate,
            chunksize: 1024,
            queuelimit: Some(4),
            capture_samplerate,
            resampler,
            capture: Device {
                dev_type: "Alsa".to_string(),
                channels,
                device: capture_device.to_string(),
                format: "S16_LE".to_string(),
            },
            playback: Device {
                dev_type: "Alsa".to_string(),
                channels,
                device: playback_device.to_string(),
                format: "S16_LE".to_string(),
            },
        },
        mixers: HashMap::new(),
        filters,
        pipeline,
    }
}

fn resample_profile_name(preset: ResamplePreset) -> &'static str {
    match preset {
        ResamplePreset::Balanced => "Balanced",
        ResamplePreset::High => "Accurate",
        ResamplePreset::Extreme => "Accurate",
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
            preamp: 0.0,
            eq_bands: vec![],
        }
    }

    // ---- device-level fields ----

    #[test]
    fn bit_perfect_uses_capture_rate_no_resampler() {
        let p = base_profile();
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.devices.samplerate, 44100);
        assert!(cfg.devices.capture_samplerate.is_none());
        assert!(cfg.devices.resampler.is_none());
    }

    #[test]
    fn resample_sets_capture_samplerate_and_resampler() {
        let mut p = base_profile();
        p.mode = DspMode::Resample;
        p.target_rate = Some(96000);
        p.preset = ResamplePreset::Balanced;
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.devices.samplerate, 96000);
        assert_eq!(cfg.devices.capture_samplerate, Some(44100));
        let resamp = cfg.devices.resampler.expect("resampler should be present");
        assert_eq!(resamp.resampler_type, "AsyncSinc");
        assert_eq!(resamp.profile, "Balanced");
    }

    #[test]
    fn resample_without_target_rate_defaults_to_capture_rate() {
        let mut p = base_profile();
        p.mode = DspMode::Resample;
        p.target_rate = None;
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.devices.samplerate, 44100);
        assert_eq!(cfg.devices.capture_samplerate, Some(44100));
        let resamp = cfg.devices.resampler.expect("resampler should be present");
        assert_eq!(resamp.profile, "Balanced");
    }

    // ---- device types and formats ----

    #[test]
    fn devices_use_alsa_type_and_underscore_format() {
        let p = base_profile();
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.devices.capture.dev_type, "Alsa");
        assert_eq!(cfg.devices.capture.format, "S16_LE");
        assert_eq!(cfg.devices.playback.dev_type, "Alsa");
        assert_eq!(cfg.devices.playback.format, "S16_LE");
    }
    #[test]
    fn bluetooth_pcm_uses_a2dp_compatible_16_bit_format() {
        let p = base_profile();
        let cfg = render_camilladsp_config(
            &p,
            "hw:Loopback,1",
            "bluealsa:DEV=AA:BB:CC:DD:EE:FF,PROFILE=a2dp",
            48000,
        );
        assert_eq!(cfg.devices.samplerate, 48000);
        assert_eq!(cfg.devices.capture.format, "S16_LE");
        assert_eq!(cfg.devices.playback.format, "S16_LE");
    }

    #[test]
    fn devices_have_chunksize_and_queuelimit() {
        let p = base_profile();
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.devices.chunksize, 1024);
        assert_eq!(cfg.devices.queuelimit, Some(4));
    }

    // ---- pipeline and filters ----

    #[test]
    fn bit_perfect_no_eq_has_empty_pipeline_and_filters() {
        let p = base_profile();
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert!(cfg.pipeline.is_empty());
        assert!(cfg.filters.is_empty());
    }

    #[test]
    fn preamp_adds_gain_filter_before_eq() {
        let mut p = base_profile();
        p.preamp = -6.5;
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.filters.len(), 2);
        assert_eq!(cfg.pipeline.len(), 2);
        assert_eq!(cfg.filters["preamp"].filter_type, "Gain");
        match &cfg.filters["preamp"].parameters {
            FilterParameters::Gain {
                gain,
                inverted,
                mute,
            } => {
                assert_eq!(*gain, -6.5);
                assert!(!inverted);
                assert!(!mute);
            }
            _ => panic!("expected Gain parameters"),
        }
        match &cfg.pipeline[0] {
            PipelineStep::Filter { channels, names } => {
                assert_eq!(channels, &vec![0u32, 1]);
                assert_eq!(names, &vec!["preamp".to_string()]);
            }
            _ => panic!("expected preamp Filter step"),
        }
    }

    #[test]
    fn eq_adds_one_filter_per_band_applied_to_both_channels() {
        // Render applies effective() — which sorts EQ bands ascending by
        // freq — so a Peaking band at 1kHz entered before a LowShelf at
        // 200Hz is rendered with the LowShelf first.
        let mut p = base_profile();
        p.eq_bands = vec![
            EqBand {
                band_type: EqBandType::Peaking,
                freq: 1000.0,
                gain: 3.0,
                q: 1.0,
            },
            EqBand {
                band_type: EqBandType::LowShelf,
                freq: 200.0,
                gain: -2.0,
                q: 0.7,
            },
        ];
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        assert_eq!(cfg.filters.len(), 2);
        assert_eq!(cfg.pipeline.len(), 2);

        // First (lowest-freq) band: Lowshelf at 200 Hz.
        let f0 = &cfg.filters["eq_band_0"];
        assert_eq!(f0.filter_type, "Biquad");
        match &f0.parameters {
            FilterParameters::Biquad {
                param_type,
                freq,
                gain,
                ..
            } => {
                assert_eq!(param_type, "Lowshelf");
                assert_eq!(*freq, 200.0);
                assert_eq!(*gain, -2.0);
            }
            _ => panic!("expected Biquad parameters"),
        }

        // Second band: Peaking at 1 kHz.
        let f1 = &cfg.filters["eq_band_1"];
        assert_eq!(f1.filter_type, "Biquad");
        match &f1.parameters {
            FilterParameters::Biquad {
                param_type,
                freq,
                gain,
                ..
            } => {
                assert_eq!(param_type, "Peaking");
                assert_eq!(*freq, 1000.0);
                assert_eq!(*gain, 3.0);
            }
            _ => panic!("expected Biquad parameters"),
        }
        // Pipeline references both bands on channels [0, 1]
        match &cfg.pipeline[0] {
            PipelineStep::Filter { channels, names } => {
                assert_eq!(channels, &vec![0u32, 1]);
                assert_eq!(names, &vec!["eq_band_0".to_string()]);
            }
            _ => panic!("expected Filter step"),
        }
    }

    #[test]
    fn every_eq_band_type_renders() {
        for band_type in [EqBandType::Peaking, EqBandType::LowShelf, EqBandType::HighShelf] {
            let mut p = base_profile();
            p.eq_bands = vec![EqBand {
                band_type,
                freq: 500.0,
                gain: -1.5,
                q: 0.9,
            }];
            let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
            assert_eq!(cfg.filters.len(), 1);
            assert_eq!(cfg.pipeline.len(), 1);
        }
    }

    #[test]
    fn every_resample_preset_maps_to_valid_v4_profile() {
        for (preset, expected) in [
            (ResamplePreset::Balanced, "Balanced"),
            (ResamplePreset::High, "Accurate"),
            (ResamplePreset::Extreme, "Accurate"),
        ] {
            let mut p = base_profile();
            p.mode = DspMode::Resample;
            p.target_rate = Some(96000);
            p.preset = preset;
            let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
            let resamp = cfg.devices.resampler.as_ref().expect("resampler");
            assert_eq!(resamp.profile, expected, "preset {preset:?}");
        }
    }

    // ---- serialisation smoke ----

    #[test]
    fn serde_roundtrip() {
        let mut p = base_profile();
        p.mode = DspMode::Resample;
        p.preamp = -6.0;
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        let cfg = render_camilladsp_config(&p, "hw:Loopback,1", "default", 44100);
        let yaml = serde_yaml::to_string(&cfg).expect("serialize");
        // Re-deserialise and verify it's structurally valid
        let _: CamillaConfig = serde_yaml::from_str(&yaml).expect("deserialize");
        // Quick sanity checks on the YAML output
        assert!(yaml.contains("type: Alsa"), "should use Alsa not Raw:\n{yaml}");
        assert!(yaml.contains("S16_LE"), "should use S16_LE:\n{yaml}");
        assert!(yaml.contains("AsyncSinc"), "should use AsyncSinc:\n{yaml}");
        assert!(yaml.contains("type: Gain"), "should render preamp Gain:\n{yaml}");
        assert!(yaml.contains("gain: -6.0"), "should include preamp gain:\n{yaml}");
        assert!(yaml.contains("chunksize: 1024"), "should have chunksize:\n{yaml}");
        assert!(yaml.contains("queuelimit:"), "should have queuelimit:\n{yaml}");
    }
}
