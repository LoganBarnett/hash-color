//! Color types and ANSI escape-code generation.

/// The terminal layer to apply color to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ground {
  /// Apply color to the text foreground. (default)
  #[default]
  Foreground,
  /// Apply color to the text background.
  Background,
}

/// A terminal color in one of the supported color spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
  /// One of the 16 standard ANSI colors (0–15).
  Ansi16(u8),
  /// One of the 256 xterm colors (0–255).
  Ansi256(u8),
  /// 24-bit true color (red, green, blue each 0–255).
  TrueColor(u8, u8, u8),
}

impl Color {
  /// Produce the ANSI escape sequence that enables this color on `ground`.
  pub fn escape_open(&self, ground: Ground) -> String {
    match (self, ground) {
      (Color::Ansi16(n), Ground::Foreground) => {
        if *n < 8 {
          format!("\x1b[{}m", 30 + n)
        } else {
          format!("\x1b[{}m", 90 + (n - 8))
        }
      }
      (Color::Ansi16(n), Ground::Background) => {
        if *n < 8 {
          format!("\x1b[{}m", 40 + n)
        } else {
          format!("\x1b[{}m", 100 + (n - 8))
        }
      }
      (Color::Ansi256(n), Ground::Foreground) => format!("\x1b[38;5;{}m", n),
      (Color::Ansi256(n), Ground::Background) => format!("\x1b[48;5;{}m", n),
      (Color::TrueColor(r, g, b), Ground::Foreground) => {
        format!("\x1b[38;2;{};{};{}m", r, g, b)
      }
      (Color::TrueColor(r, g, b), Ground::Background) => {
        format!("\x1b[48;2;{};{};{}m", r, g, b)
      }
    }
  }

  /// The ANSI escape sequence that resets all SGR attributes.
  pub fn escape_close() -> &'static str {
    "\x1b[0m"
  }
}

/// Convert HSL (hue 0–360, saturation 0–1, lightness 0–1) to sRGB (each
/// component 0–255).
pub fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> (u8, u8, u8) {
  let s = saturation.clamp(0.0, 1.0);
  let l = lightness.clamp(0.0, 1.0);
  let h = hue.rem_euclid(360.0);

  let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
  let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
  let m = l - c / 2.0;

  let (r1, g1, b1) = if h < 60.0 {
    (c, x, 0.0)
  } else if h < 120.0 {
    (x, c, 0.0)
  } else if h < 180.0 {
    (0.0, c, x)
  } else if h < 240.0 {
    (0.0, x, c)
  } else if h < 300.0 {
    (x, 0.0, c)
  } else {
    (c, 0.0, x)
  };

  (
    ((r1 + m) * 255.0).round() as u8,
    ((g1 + m) * 255.0).round() as u8,
    ((b1 + m) * 255.0).round() as u8,
  )
}

/// Compute the approximate hue (0–360°) of an sRGB color.
///
/// Returns 0.0 for achromatic inputs (r == g == b).
pub fn rgb_to_hue(r: u8, g: u8, b: u8) -> f32 {
  let r = r as f32 / 255.0;
  let g = g as f32 / 255.0;
  let b = b as f32 / 255.0;

  let max = r.max(g).max(b);
  let min = r.min(g).min(b);
  let delta = max - min;

  if delta < f32::EPSILON {
    return 0.0; // achromatic
  }

  let hue = if max == r {
    60.0 * (((g - b) / delta) % 6.0)
  } else if max == g {
    60.0 * ((b - r) / delta + 2.0)
  } else {
    60.0 * ((r - g) / delta + 4.0)
  };

  hue.rem_euclid(360.0)
}

/// Convert a 6×6×6 xterm-256 cube index (0–215) to sRGB components.
///
/// Each channel value `n` maps to 0 when n == 0, otherwise 55 + 40 × n.
pub fn cube_index_to_rgb(cube_idx: u8) -> (u8, u8, u8) {
  let r_idx = cube_idx / 36;
  let g_idx = (cube_idx % 36) / 6;
  let b_idx = cube_idx % 6;

  let channel = |n: u8| if n == 0 { 0u8 } else { 55 + 40 * n };
  (channel(r_idx), channel(g_idx), channel(b_idx))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hsl_red_is_rgb_red() {
    let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
    assert_eq!((r, g, b), (255, 0, 0));
  }

  #[test]
  fn hsl_green_is_rgb_green() {
    let (r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5);
    assert_eq!((r, g, b), (0, 255, 0));
  }

  #[test]
  fn hsl_blue_is_rgb_blue() {
    let (r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5);
    assert_eq!((r, g, b), (0, 0, 255));
  }

  #[test]
  fn cube_index_zero_is_black() {
    assert_eq!(cube_index_to_rgb(0), (0, 0, 0));
  }

  #[test]
  fn ansi16_foreground_codes() {
    assert_eq!(Color::Ansi16(0).escape_open(Ground::Foreground), "\x1b[30m");
    assert_eq!(Color::Ansi16(8).escape_open(Ground::Foreground), "\x1b[90m");
  }

  #[test]
  fn ansi256_background_code() {
    assert_eq!(
      Color::Ansi256(200).escape_open(Ground::Background),
      "\x1b[48;5;200m"
    );
  }

  #[test]
  fn truecolor_foreground_code() {
    assert_eq!(
      Color::TrueColor(255, 128, 0).escape_open(Ground::Foreground),
      "\x1b[38;2;255;128;0m"
    );
  }
}
