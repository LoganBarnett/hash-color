//! Hash-to-color mapping and text colorization.
//!
//! The primary entry point is [`HashColorizer`], which maps arbitrary byte
//! sequences to stable ANSI-colored text via [`fnv1a_64_seeded`].

use crate::color::{cube_index_to_rgb, hsl_to_rgb, rgb_to_hue, Color, Ground};
use crate::detect::{detect_color_support, ColorSupport};
use crate::exclusion::HueExclusion;
use crate::hash::fnv1a_64_seeded;

/// Options controlling how [`HashColorizer`] selects colors.
#[derive(Debug, Clone)]
pub struct ColorizerOptions {
  /// Color support override.  `None` means auto-detect from the environment.
  pub color_support: Option<ColorSupport>,

  /// Apply color to the text foreground or background.
  pub ground: Ground,

  /// A seed mixed into every hash computation, shifting the color assigned to
  /// each input.  Increment this to resolve unwanted color collisions without
  /// changing the input text.
  pub seed: u64,

  /// Hue arcs (degrees, 0–360) to avoid when selecting colors.
  ///
  /// Arcs from color-blindness presets and from `--exclude-hue` flags are
  /// merged into this list before the colorizer is constructed.
  pub hue_exclusions: Vec<HueExclusion>,

  /// Saturation applied when generating true-color output (0.0–1.0).
  ///
  /// Values closer to 1.0 give vivid colors; 0.0 produces grayscale.
  pub saturation: f32,

  /// Lightness applied when generating true-color output (0.0–1.0).
  ///
  /// `None` picks a default based on [`ground`](Self::ground):
  /// - `0.60` for foreground (bright enough on dark backgrounds).
  /// - `0.35` for background (dark enough that light text is legible).
  pub lightness: Option<f32>,
}

impl Default for ColorizerOptions {
  fn default() -> Self {
    Self {
      color_support: None,
      ground: Ground::Foreground,
      seed: 0,
      hue_exclusions: vec![],
      saturation: 0.7,
      lightness: None,
    }
  }
}

/// Text paired with the ANSI escape sequences that apply its hash-derived
/// color.
///
/// Implements [`Display`](std::fmt::Display): printing a `ColorizedText`
/// emits the opening escape sequence, the text, and the reset sequence.
/// When color support is [`None`](ColorSupport::None), `open` and `close`
/// are empty so the output is identical to the undecorated text.
#[derive(Debug, Clone)]
pub struct ColorizedText {
  /// The original (unescaped) text.
  pub text: String,
  /// ANSI sequence to enable the chosen color (may be empty).
  pub open: String,
  /// ANSI sequence to reset attributes (may be empty).
  pub close: String,
}

impl std::fmt::Display for ColorizedText {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}{}{}", self.open, self.text, self.close)
  }
}

/// Colorizes text using a hash of the input to select a stable color.
///
/// The same input always produces the same color for a given configuration.
/// Changing [`seed`](ColorizerOptions::seed) shifts the mapping so different
/// inputs can be distinguished when they would otherwise collide.
///
/// # Example
///
/// ```
/// use hash_color_lib::{HashColorizer, ColorizerOptions, ColorSupport};
///
/// let mut opts = ColorizerOptions::default();
/// opts.color_support = Some(ColorSupport::TrueColor);
///
/// let colorizer = HashColorizer::new(opts);
/// let colored = colorizer.colorize("my-hostname");
/// println!("{}", colored); // "my-hostname" in a consistent color
/// ```
pub struct HashColorizer {
  options: ColorizerOptions,
  effective_support: ColorSupport,
}

impl HashColorizer {
  /// Construct a new colorizer, auto-detecting color support if not overridden
  /// in `options`.
  pub fn new(options: ColorizerOptions) -> Self {
    let effective_support =
      options.color_support.unwrap_or_else(detect_color_support);
    Self {
      options,
      effective_support,
    }
  }

  /// Compute the [`Color`] that corresponds to `input`.
  ///
  /// Returns `None` when the effective color support level is
  /// [`ColorSupport::None`].
  pub fn color_for(&self, input: &[u8]) -> Option<Color> {
    let hash = fnv1a_64_seeded(input, self.options.seed);
    match self.effective_support {
      ColorSupport::None => None,
      ColorSupport::Ansi16 => Some(self.hash_to_ansi16(hash)),
      ColorSupport::Ansi256 => Some(self.hash_to_ansi256(hash)),
      ColorSupport::TrueColor => Some(self.hash_to_truecolor(hash)),
    }
  }

  /// Wrap `text` in ANSI escapes chosen by hashing `key`.
  ///
  /// This lets the *display text* differ from the *hash key*.  For example,
  /// when coloring a hostname that appears embedded in a longer line, pass the
  /// bare hostname as `key` so the color stays stable regardless of the
  /// surrounding context.
  pub fn colorize_with_key(&self, key: &[u8], text: &str) -> ColorizedText {
    match self.color_for(key) {
      Some(color) => ColorizedText {
        text: text.to_string(),
        open: color.escape_open(self.options.ground),
        close: Color::escape_close().to_string(),
      },
      None => ColorizedText {
        text: text.to_string(),
        open: String::new(),
        close: String::new(),
      },
    }
  }

  /// Hash `text` and wrap it in the corresponding ANSI color escapes.
  pub fn colorize(&self, text: &str) -> ColorizedText {
    self.colorize_with_key(text.as_bytes(), text)
  }

  /// Hash `bytes` and wrap the UTF-8 representation in ANSI color escapes.
  ///
  /// Non-UTF-8 bytes are replaced with U+FFFD.
  pub fn colorize_bytes(&self, bytes: &[u8]) -> ColorizedText {
    let text = String::from_utf8_lossy(bytes).into_owned();
    self.colorize_with_key(bytes, &text)
  }

  // ─── private helpers ──────────────────────────────────────────────────────

  fn hash_to_ansi16(&self, hash: u64) -> Color {
    // Palette: (color_index, approximate_hue).  0 (black) and 8 (dark grey)
    // are omitted — they are typically unreadable against common backgrounds.
    const PALETTE: &[(u8, Option<f32>)] = &[
      (1, Some(0.0)),    // red
      (2, Some(120.0)),  // green
      (3, Some(60.0)),   // yellow
      (4, Some(240.0)),  // blue
      (5, Some(300.0)),  // magenta
      (6, Some(180.0)),  // cyan
      (7, None),         // white — achromatic; always included
      (9, Some(0.0)),    // bright red
      (10, Some(120.0)), // bright green
      (11, Some(60.0)),  // bright yellow
      (12, Some(240.0)), // bright blue
      (13, Some(300.0)), // bright magenta
      (14, Some(180.0)), // bright cyan
      (15, None),        // bright white — achromatic; always included
    ];

    let grayscale_only = self.all_hues_excluded();

    let candidates: Vec<u8> = PALETTE
      .iter()
      .filter(|&&(_n, hue_opt)| {
        match hue_opt {
          // Achromatic entries (white/bright-white) are always included unless
          // all hues happen to be excluded, in which case we *only* want them.
          None => true,
          Some(hue) => !grayscale_only && !self.is_hue_excluded(hue),
        }
      })
      .map(|&(n, _)| n)
      .collect();

    let n = candidates[hash as usize % candidates.len()];
    Color::Ansi16(n)
  }

  fn hash_to_ansi256(&self, hash: u64) -> Color {
    if self.all_hues_excluded() {
      // Grayscale ramp: xterm indices 232–255 (24 shades)
      return Color::Ansi256(232 + (hash % 24) as u8);
    }

    // Walk the 6×6×6 color cube (cube indices 0–215 → xterm indices 16–231)
    // starting from the hash-derived position, picking the first cube entry
    // whose hue is not excluded.
    for offset in 0u64..216 {
      let cube_idx = ((hash + offset) % 216) as u8;
      let (r, g, b) = cube_index_to_rgb(cube_idx);

      // Near-achromatic cube cells have no meaningful hue; always allow them.
      let r_f = r as f32 / 255.0;
      let g_f = g as f32 / 255.0;
      let b_f = b as f32 / 255.0;
      let is_near_achromatic =
        (r_f.max(g_f).max(b_f) - r_f.min(g_f).min(b_f)) < 0.15;

      let hue = rgb_to_hue(r, g, b);
      if is_near_achromatic || !self.is_hue_excluded(hue) {
        return Color::Ansi256(16 + cube_idx);
      }
    }

    // All cube entries were excluded — fall back to grayscale
    Color::Ansi256(232 + (hash % 24) as u8)
  }

  fn hash_to_truecolor(&self, hash: u64) -> Color {
    if self.all_hues_excluded() {
      // Grayscale: bias brightness toward the readable middle range (60–220).
      let v = 60u8.saturating_add((hash % 160) as u8);
      return Color::TrueColor(v, v, v);
    }

    let hue = self.map_hash_to_allowed_hue(hash);
    let s = self.options.saturation;
    let l =
      self
        .options
        .lightness
        .unwrap_or_else(|| match self.options.ground {
          Ground::Foreground => 0.60,
          Ground::Background => 0.35,
        });

    let (r, g, b) = hsl_to_rgb(hue, s, l);
    Color::TrueColor(r, g, b)
  }

  /// Map a raw hash value uniformly to a hue that lies within one of the
  /// allowed (non-excluded) arcs of the color wheel.
  fn map_hash_to_allowed_hue(&self, hash: u64) -> f32 {
    let allowed = self.allowed_hue_ranges();
    let total: f32 = allowed.iter().map(|(s, e)| e - s).sum();

    if total <= 0.0 {
      return 0.0;
    }

    // Use f64 for the large-integer division to preserve precision.
    let position = (hash as f64 / u64::MAX as f64) as f32 * total;

    let mut remaining = position;
    for (start, end) in allowed {
      let span = end - start;
      if remaining < span {
        return start + remaining;
      }
      remaining -= span;
    }
    0.0
  }

  /// Return the arcs of the hue wheel that are *not* excluded.
  ///
  /// The result is a list of `(start, end)` pairs in ascending order.
  fn allowed_hue_ranges(&self) -> Vec<(f32, f32)> {
    if self.options.hue_exclusions.is_empty() {
      return vec![(0.0, 360.0)];
    }

    // Normalize every exclusion to non-wrapping sub-intervals in [0, 360).
    let mut excluded: Vec<(f32, f32)> = Vec::new();
    for ex in &self.options.hue_exclusions {
      // A range whose width is >= 360° covers the full circle.
      if (ex.end_deg - ex.start_deg).abs() >= 360.0 {
        return vec![]; // No allowed hues at all
      }
      let s = ex.start_deg.rem_euclid(360.0);
      let e = ex.end_deg.rem_euclid(360.0);
      if s < e {
        excluded.push((s, e));
      } else if s > e {
        // Wrapping arc — split into two non-wrapping intervals
        excluded.push((s, 360.0));
        excluded.push((0.0, e));
      }
      // s == e (zero-width after normalization) → skip
    }

    // Sort then merge overlapping intervals
    excluded.sort_by(|a, b| {
      a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (s, e) in excluded {
      if let Some(last) = merged.last_mut() {
        if s <= last.1 {
          last.1 = last.1.max(e);
          continue;
        }
      }
      merged.push((s, e));
    }

    // Invert: collect the gaps between merged exclusions
    let mut allowed: Vec<(f32, f32)> = Vec::new();
    let mut pos = 0.0f32;
    for (s, e) in merged {
      if pos < s {
        allowed.push((pos, s));
      }
      pos = e.max(pos);
    }
    if pos < 360.0 {
      allowed.push((pos, 360.0));
    }

    allowed
  }

  fn is_hue_excluded(&self, hue: f32) -> bool {
    self
      .options
      .hue_exclusions
      .iter()
      .any(|ex| ex.contains(hue))
  }

  fn all_hues_excluded(&self) -> bool {
    self
      .allowed_hue_ranges()
      .iter()
      .map(|(s, e)| e - s)
      .sum::<f32>()
      < 1.0
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::exclusion::HueExclusion;

  fn truecolor_opts() -> ColorizerOptions {
    ColorizerOptions {
      color_support: Some(ColorSupport::TrueColor),
      ..Default::default()
    }
  }

  #[test]
  fn same_input_same_color() {
    let c = HashColorizer::new(truecolor_opts());
    assert_eq!(c.color_for(b"hello"), c.color_for(b"hello"));
  }

  #[test]
  fn different_inputs_usually_different_colors() {
    let c = HashColorizer::new(truecolor_opts());
    // Not guaranteed, but astronomically unlikely to collide for these two
    assert_ne!(c.color_for(b"foo"), c.color_for(b"bar"));
  }

  #[test]
  fn seed_changes_color() {
    let c1 = HashColorizer::new(truecolor_opts());
    let c2 = HashColorizer::new(ColorizerOptions {
      seed: 42,
      ..truecolor_opts()
    });
    assert_ne!(c1.color_for(b"hello"), c2.color_for(b"hello"));
  }

  #[test]
  fn no_color_support_returns_none() {
    let c = HashColorizer::new(ColorizerOptions {
      color_support: Some(ColorSupport::None),
      ..Default::default()
    });
    assert_eq!(c.color_for(b"hello"), None);
  }

  #[test]
  fn display_no_color_is_plain_text() {
    let c = HashColorizer::new(ColorizerOptions {
      color_support: Some(ColorSupport::None),
      ..Default::default()
    });
    let ct = c.colorize("hello");
    assert_eq!(ct.to_string(), "hello");
  }

  #[test]
  fn achromatopsia_gives_truecolor_gray() {
    use crate::exclusion::ColorBlindnessMode;
    let opts = ColorizerOptions {
      color_support: Some(ColorSupport::TrueColor),
      hue_exclusions: ColorBlindnessMode::Achromatopsia.hue_exclusions(),
      ..Default::default()
    };
    let c = HashColorizer::new(opts);
    if let Some(Color::TrueColor(r, g, b)) = c.color_for(b"test") {
      assert_eq!(r, g);
      assert_eq!(g, b);
    } else {
      panic!("expected TrueColor");
    }
  }

  #[test]
  fn hue_exclusion_avoids_excluded_hue() {
    // Exclude everything except the red hue band (0°–60° and 300°–360°)
    // by excluding 60°–300°.
    let opts = ColorizerOptions {
      color_support: Some(ColorSupport::TrueColor),
      hue_exclusions: vec![HueExclusion::new(60.0, 300.0)],
      ..Default::default()
    };
    let c = HashColorizer::new(opts);
    // Run many inputs and verify no selected hue falls in the exclusion
    for i in 0u64..50 {
      let input = i.to_string();
      if let Some(Color::TrueColor(r, g, b)) = c.color_for(input.as_bytes()) {
        let hue = crate::color::rgb_to_hue(r, g, b);
        assert!(
          !(60.0..300.0).contains(&hue),
          "hue {hue} fell in excluded range for input {input}"
        );
      }
    }
  }
}
