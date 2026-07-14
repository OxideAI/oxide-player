use serde::{Deserialize, Serialize};

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
