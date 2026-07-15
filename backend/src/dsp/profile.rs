use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// Standard audio output sample rates accepted by CamillaDSP / DACs.
pub const STANDARD_RATES: &[u32] = &[44100, 48000, 88200, 96000, 176400, 192000];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DspMode {
    BitPerfect,
    Resample,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResamplePreset {
    #[default]
    Balanced,
    High,
    Extreme,
}

impl ResamplePreset {
    pub fn sox_quality(&self) -> u8 {
        match self {
            ResamplePreset::Balanced => 3,
            ResamplePreset::High => 5,
            ResamplePreset::Extreme => 7,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EqBandType {
    Peaking,
    LowShelf,
    HighShelf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EqBand {
    #[serde(rename = "type")]
    pub band_type: EqBandType,
    pub freq: f64,
    pub gain: f64,
    pub q: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DspProfile {
    pub device: String,
    pub mode: DspMode,
    #[serde(default)]
    pub target_rate: Option<u32>,
    #[serde(default)]
    pub preset: ResamplePreset,
    #[serde(default)]
    pub eq_bands: Vec<EqBand>,
}

impl DspProfile {
    pub fn bit_perfect(device: &str) -> Self {
        DspProfile {
            device: device.to_string(),
            mode: DspMode::BitPerfect,
            target_rate: None,
            preset: ResamplePreset::default(),
            eq_bands: Vec::new(),
        }
    }

    /// Validate the profile numerics before they reach CamillaDSP.
    ///
    /// Returns a clear, user-facing message on the first problem found.
    pub fn validate(&self) -> Result<()> {
        if self.device.trim().is_empty()
            || self.device.contains('\n')
            || self.device.contains('\0')
        {
            bail!("invalid dsp device name: {:?}", self.device);
        }

        if let Some(rate) = self.target_rate {
            if !STANDARD_RATES.contains(&rate) {
                bail!(
                    "target_rate must be a standard rate: {}",
                    STANDARD_RATES
                        .iter()
                        .map(|r| r.to_string())
                        .collect::<Vec<_>>()
                        .join("/")
                );
            }
        }

        for (i, band) in self.eq_bands.iter().enumerate() {
            if band.freq <= 0.0 || band.freq >= 200_000.0 {
                bail!("eq band {i} freq must be between 1 and 200000 Hz");
            }
            if band.gain.abs() > 30.0 {
                bail!("eq band {i} gain must be within ±30 dB");
            }
            if band.q <= 0.0 {
                bail!("eq band {i} q must be > 0");
            }
        }

        Ok(())
    }

    /// Normalize the profile so bit-perfect mode never applies DSP (R10).
    pub fn effective(&self) -> DspProfile {
        if self.mode == DspMode::BitPerfect {
            DspProfile {
                device: self.device.clone(),
                mode: DspMode::BitPerfect,
                target_rate: None,
                preset: ResamplePreset::default(),
                eq_bands: Vec::new(),
            }
        } else {
            self.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(device: &str) -> DspProfile {
        DspProfile {
            device: device.to_string(),
            mode: DspMode::BitPerfect,
            target_rate: None,
            preset: ResamplePreset::default(),
            eq_bands: vec![],
        }
    }

    #[test]
    fn empty_device_is_rejected() {
        assert!(base("").validate().is_err());
    }

    #[test]
    fn valid_profile_passes() {
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(96000);
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 3.0,
            q: 1.0,
        }];
        assert!(p.validate().is_ok());
    }

    #[test]
    fn non_standard_rate_is_rejected() {
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(50000);
        let err = p.validate().err().unwrap().to_string();
        assert!(err.contains("standard rate"), "got: {err}");
    }

    #[test]
    fn eq_band_q_must_be_positive() {
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(48000);
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 0.0,
            q: 0.0,
        }];
        let err = p.validate().err().unwrap().to_string();
        assert!(err.contains("q must be > 0"), "got: {err}");
    }

    #[test]
    fn eq_band_gain_is_clamped() {
        let mut p = base("DAC");
        p.mode = DspMode::Resample;
        p.target_rate = Some(48000);
        p.eq_bands = vec![EqBand {
            band_type: EqBandType::Peaking,
            freq: 1000.0,
            gain: 40.0,
            q: 1.0,
        }];
        assert!(p.validate().is_err());
    }
}
