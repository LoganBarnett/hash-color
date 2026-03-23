//! Hue exclusion ranges and color-blindness accommodation presets.

use std::str::FromStr;
use thiserror::Error;

/// A contiguous arc of hues (in degrees, 0–360) to avoid when selecting
/// colors.
///
/// When `start_deg <= end_deg` the arc covers `[start_deg, end_deg)`.
/// When `start_deg > end_deg` the arc wraps around 0°/360° and covers
/// `[start_deg, 360) ∪ [0, end_deg)`.
#[derive(Debug, Clone)]
pub struct HueExclusion {
  /// Start of the excluded arc in degrees (inclusive).
  pub start_deg: f32,
  /// End of the excluded arc in degrees (exclusive).
  pub end_deg: f32,
}

impl HueExclusion {
  pub fn new(start_deg: f32, end_deg: f32) -> Self {
    Self { start_deg, end_deg }
  }

  /// Return `true` if `hue` (0–360) falls within this exclusion arc.
  pub fn contains(&self, hue: f32) -> bool {
    let h = hue.rem_euclid(360.0);
    if self.start_deg <= self.end_deg {
      h >= self.start_deg && h < self.end_deg
    } else {
      // Arc wraps around 0°/360°
      h >= self.start_deg || h < self.end_deg
    }
  }
}

/// Parse `"start:end"` (e.g. `"60:180"` or `"340:30"`).
impl FromStr for HueExclusion {
  type Err = HueExclusionParseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (start_str, end_str) = s
      .split_once(':')
      .ok_or_else(|| HueExclusionParseError::BadFormat(s.to_string()))?;

    let start = start_str.trim().parse::<f32>().map_err(|source| {
      HueExclusionParseError::BadDegree {
        input: s.to_string(),
        source,
      }
    })?;

    let end = end_str.trim().parse::<f32>().map_err(|source| {
      HueExclusionParseError::BadDegree {
        input: s.to_string(),
        source,
      }
    })?;

    Ok(HueExclusion::new(start, end))
  }
}

#[derive(Debug, Error)]
pub enum HueExclusionParseError {
  #[error(
    "Invalid hue exclusion {0:?}: expected START:END where both are \
     degrees in 0–360 (e.g. \"60:180\")"
  )]
  BadFormat(String),

  #[error("Invalid degree value in hue exclusion {input:?}: {source}")]
  BadDegree {
    input: String,
    #[source]
    source: std::num::ParseFloatError,
  },
}

/// Preset color-blindness accommodation modes.
///
/// Each mode expands to a set of [`HueExclusion`] arcs covering the hues that
/// are most difficult to distinguish for that condition, leaving the remaining
/// portion of the color wheel for selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorBlindnessMode {
  /// Deuteranopia / deuteranomaly — green-weak red–green color deficiency.
  ///
  /// Excludes greens (~80°–170°) which are confused with reds and yellows.
  Deuteranopia,

  /// Protanopia / protanomaly — red-weak red–green color deficiency.
  ///
  /// Excludes reds and magentas (~340°–30°, wrapping around 0°).
  Protanopia,

  /// Tritanopia / tritanomaly — blue–yellow color deficiency.
  ///
  /// Excludes blues and cyans (~180°–270°).
  Tritanopia,

  /// Achromatopsia — complete color blindness; only brightness is perceived.
  ///
  /// Covers the full 0°–360° arc; the colorizer interprets this as a request
  /// for grayscale output regardless of the terminal's color support level.
  Achromatopsia,
}

impl ColorBlindnessMode {
  /// Return the hue exclusion arcs for this accommodation mode.
  pub fn hue_exclusions(&self) -> Vec<HueExclusion> {
    match self {
      ColorBlindnessMode::Deuteranopia => {
        vec![HueExclusion::new(80.0, 170.0)]
      }
      ColorBlindnessMode::Protanopia => {
        // Wraps around: covers 340°–360° and 0°–30°
        vec![HueExclusion::new(340.0, 30.0)]
      }
      ColorBlindnessMode::Tritanopia => {
        vec![HueExclusion::new(180.0, 270.0)]
      }
      ColorBlindnessMode::Achromatopsia => {
        vec![HueExclusion::new(0.0, 360.0)]
      }
    }
  }
}

impl std::fmt::Display for ColorBlindnessMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ColorBlindnessMode::Deuteranopia => write!(f, "deuteranopia"),
      ColorBlindnessMode::Protanopia => write!(f, "protanopia"),
      ColorBlindnessMode::Tritanopia => write!(f, "tritanopia"),
      ColorBlindnessMode::Achromatopsia => write!(f, "achromatopsia"),
    }
  }
}

impl FromStr for ColorBlindnessMode {
  type Err = ColorBlindnessModeParseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "deuteranopia" | "deuteranomaly" | "green-weak" => {
        Ok(ColorBlindnessMode::Deuteranopia)
      }
      "protanopia" | "protanomaly" | "red-weak" => {
        Ok(ColorBlindnessMode::Protanopia)
      }
      "tritanopia" | "tritanomaly" | "blue-yellow" => {
        Ok(ColorBlindnessMode::Tritanopia)
      }
      "achromatopsia" | "monochromacy" | "grayscale" | "greyscale" => {
        Ok(ColorBlindnessMode::Achromatopsia)
      }
      _ => Err(ColorBlindnessModeParseError::Unknown(s.to_string())),
    }
  }
}

#[derive(Debug, Error)]
pub enum ColorBlindnessModeParseError {
  #[error(
    "Unknown color-blindness mode: {0}. \
     Valid values: deuteranopia, protanopia, tritanopia, achromatopsia"
  )]
  Unknown(String),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn exclusion_normal_range() {
    let ex = HueExclusion::new(60.0, 180.0);
    assert!(ex.contains(90.0));
    assert!(!ex.contains(200.0));
    assert!(!ex.contains(50.0));
  }

  #[test]
  fn exclusion_wrapping_range() {
    // 340°–30° wraps around 0°
    let ex = HueExclusion::new(340.0, 30.0);
    assert!(ex.contains(350.0));
    assert!(ex.contains(10.0));
    assert!(!ex.contains(90.0));
  }

  #[test]
  fn parse_exclusion_from_str() {
    let ex: HueExclusion = "60:180".parse().unwrap();
    assert_eq!(ex.start_deg, 60.0);
    assert_eq!(ex.end_deg, 180.0);
  }

  #[test]
  fn color_blind_modes_parse() {
    assert_eq!(
      "deuteranopia".parse::<ColorBlindnessMode>().unwrap(),
      ColorBlindnessMode::Deuteranopia
    );
    assert_eq!(
      "achromatopsia".parse::<ColorBlindnessMode>().unwrap(),
      ColorBlindnessMode::Achromatopsia
    );
  }
}
