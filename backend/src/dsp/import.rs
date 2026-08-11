use crate::dsp::profile::{DspMode, DspProfile, EqBand, EqBandType, ResamplePreset};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const MAX_IMPORT_BYTES: usize = 1024 * 1024;

/// The subset of an AutoEQ/Equalizer APO-style file that Oxide imports.
/// Everything except `Preamp:` and numbered `Filter` lines is ignored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DspSettings {
    pub preamp: f64,
    pub eq_bands: Vec<EqBand>,
}

/// Parse AutoEQ-style text such as:
/// `Preamp: -0.7 dB` and `Filter 1: ON PK Fc 20 Hz Gain +0.00 dB Q 4.32`.
///
/// Filter numbers determine the imported order. The renderer later applies its
/// normal ascending-frequency canonicalization.
pub fn parse_dsp_text(text: &str) -> Result<DspSettings> {
    if text.len() > MAX_IMPORT_BYTES {
        bail!("DSP import is too large (maximum is {MAX_IMPORT_BYTES} bytes)");
    }

    let mut preamp = 0.0;
    let mut saw_preamp = false;
    let mut filters = BTreeMap::new();
    let mut saw_setting = false;

    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("Preamp:") {
            if saw_preamp {
                bail!("line {line_number}: duplicate Preamp value");
            }
            preamp = parse_number(value, "Preamp", line_number)?;
            saw_preamp = true;
            saw_setting = true;
            continue;
        }

        let Some(filter_tail) = trimmed.strip_prefix("Filter") else {
            continue;
        };
        let Some(first) = filter_tail.chars().next() else {
            continue;
        };
        if !first.is_ascii_whitespace() {
            continue;
        }

        let (number, values) = parse_filter_header(filter_tail, line_number)?;
        if filters
            .insert(number, parse_filter_values(values, line_number)?)
            .is_some()
        {
            bail!("line {line_number}: duplicate Filter {number}");
        }
        saw_setting = true;
    }

    if !saw_setting {
        bail!("DSP import contains no Preamp or numbered Filter values");
    }

    let settings = DspSettings {
        preamp,
        eq_bands: filters.into_values().collect(),
    };
    validate_settings(&settings)?;
    Ok(settings)
}

fn parse_filter_header(tail: &str, line_number: usize) -> Result<(u32, &str)> {
    let tail = tail.trim_start();
    let digits_end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    if digits_end == 0 {
        bail!("line {line_number}: Filter must have a numeric filter number");
    }
    let number = tail[..digits_end]
        .parse::<u32>()
        .with_context(|| format!("line {line_number}: invalid Filter number"))?;
    let after_number = tail[digits_end..].trim_start();
    let values = after_number
        .strip_prefix(':')
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: Filter is missing ':'"))?;
    Ok((number, values.trim()))
}

fn parse_filter_values(values: &str, line_number: usize) -> Result<EqBand> {
    let mut tokens = values.split_whitespace();
    let first = tokens
        .next()
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: Filter has no values"))?;
    let filter_type = if first.eq_ignore_ascii_case("on")
        || first.eq_ignore_ascii_case("off")
    {
        tokens
            .next()
            .ok_or_else(|| anyhow::anyhow!("line {line_number}: Filter is missing its type"))?
    } else {
        first
    };
    let band_type = match filter_type.to_ascii_uppercase().as_str() {
        "PK" | "PEAKING" => EqBandType::Peaking,
        "LS" | "LOWSHELF" | "LOW_SHELF" => EqBandType::LowShelf,
        "HS" | "HIGHSHELF" | "HIGH_SHELF" => EqBandType::HighShelf,
        _ => bail!("line {line_number}: unsupported Filter type {filter_type:?}"),
    };

    let freq = parse_named_number(&mut tokens, "Fc", line_number)?;
    let gain = parse_named_number(&mut tokens, "Gain", line_number)?;
    let q = parse_named_number(&mut tokens, "Q", line_number)?;
    Ok(EqBand {
        band_type,
        freq,
        gain,
        q,
    })
}

fn parse_named_number<'a>(
    tokens: &mut impl Iterator<Item = &'a str>,
    name: &str,
    line_number: usize,
) -> Result<f64> {
    while let Some(token) = tokens.next() {
        if token.eq_ignore_ascii_case(name) {
            let value = tokens.next().ok_or_else(|| {
                anyhow::anyhow!("line {line_number}: {name} is missing a numeric value")
            })?;
            return value.parse::<f64>().with_context(|| {
                format!("line {line_number}: {name} value {value:?} is not numeric")
            });
        }
    }
    bail!("line {line_number}: Filter is missing {name}")
}

fn parse_number(value: &str, name: &str, line_number: usize) -> Result<f64> {
    let token = value
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow::anyhow!("line {line_number}: {name} is missing a numeric value"))?;
    token.parse::<f64>().with_context(|| {
        format!("line {line_number}: {name} value {token:?} is not numeric")
    })
}

fn validate_settings(settings: &DspSettings) -> Result<()> {
    let profile = DspProfile {
        device: "import".to_string(),
        mode: DspMode::BitPerfect,
        target_rate: None,
        preset: ResamplePreset::default(),
        preamp: settings.preamp,
        eq_bands: settings.eq_bands.clone(),
    };
    profile.validate().context("invalid imported DSP values")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spinorama_auto_eq_file() {
        let text = r#"
            EQ for a speaker
            Preference Score 5.98

            Preamp: -0.7 dB

            Filter  2: ON PK Fc 25 Hz Gain +0.50 dB Q 4.32
            Filter  1: ON PK Fc 20 Hz Gain +0.00 dB Q 4.32
            Filter  3: ON PK Fc 31 Hz Gain -0.25 dB Q 4.32
        "#;
        let parsed = parse_dsp_text(text).unwrap();
        assert_eq!(parsed.preamp, -0.7);
        assert_eq!(parsed.eq_bands.len(), 3);
        assert_eq!(parsed.eq_bands[0].freq, 20.0);
        assert_eq!(parsed.eq_bands[1].gain, 0.5);
        assert_eq!(parsed.eq_bands[2].q, 4.32);
    }

    #[test]
    fn parses_shelf_aliases_and_ignores_unrelated_lines() {
        let text = "# ignored\nPreamp: 1.5 dB\nFilter 1: ON LS Fc 100 Hz Gain -2 dB Q 0.7\nFilter 2: ON HS Fc 8000 Hz Gain 3 dB Q 0.8\n";
        let parsed = parse_dsp_text(text).unwrap();
        assert_eq!(parsed.preamp, 1.5);
        assert_eq!(parsed.eq_bands[0].band_type, EqBandType::LowShelf);
        assert_eq!(parsed.eq_bands[1].band_type, EqBandType::HighShelf);
    }

    #[test]
    fn preamp_only_is_a_valid_import() {
        let parsed = parse_dsp_text("Preamp: -6 dB\n").unwrap();
        assert_eq!(parsed.preamp, -6.0);
        assert!(parsed.eq_bands.is_empty());
    }

    #[test]
    fn malformed_filter_is_rejected() {
        let error = parse_dsp_text("Filter 1: ON PK Fc 20 Hz Gain 1 dB\n")
            .unwrap_err()
            .to_string();
        assert!(error.contains("missing Q"), "got: {error}");
    }

    #[test]
    fn duplicate_filter_number_is_rejected() {
        let text = "Filter 1: ON PK Fc 20 Hz Gain 0 dB Q 1\nFilter 1: ON PK Fc 30 Hz Gain 0 dB Q 1\n";
        let error = parse_dsp_text(text).unwrap_err().to_string();
        assert!(error.contains("duplicate Filter 1"), "got: {error}");
    }
}
