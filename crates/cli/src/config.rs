use clap::Parser;
use hash_color_lib::{
  ColorBlindnessMode, ColorSupport, ColorizerOptions, Ground, HueExclusion,
  LogFormat, LogLevel,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("Configuration validation failed: {0}")]
  Validation(String),
}

/// Color the foreground or background of the output.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum GroundArg {
  /// Color the text foreground.
  #[value(alias = "fg", alias = "foreground")]
  Fg,
  /// Color the text background.
  #[value(alias = "bg", alias = "background")]
  Bg,
}

impl From<GroundArg> for Ground {
  fn from(g: GroundArg) -> Ground {
    match g {
      GroundArg::Fg => Ground::Foreground,
      GroundArg::Bg => Ground::Background,
    }
  }
}

/// Override automatic color-support detection.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ColorSupportArg {
  /// Disable all color output.
  #[value(name = "none")]
  None,
  /// 16 standard ANSI colors.
  #[value(name = "16", alias = "ansi16", alias = "ansi")]
  Ansi16,
  /// 256-color xterm palette.
  #[value(name = "256", alias = "ansi256")]
  Ansi256,
  /// 24-bit true color.
  #[value(name = "truecolor", alias = "24bit")]
  TrueColor,
}

impl From<ColorSupportArg> for ColorSupport {
  fn from(c: ColorSupportArg) -> ColorSupport {
    match c {
      ColorSupportArg::None => ColorSupport::None,
      ColorSupportArg::Ansi16 => ColorSupport::Ansi16,
      ColorSupportArg::Ansi256 => ColorSupport::Ansi256,
      ColorSupportArg::TrueColor => ColorSupport::TrueColor,
    }
  }
}

/// Preset for accommodating a color-vision deficiency.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ColorBlindArg {
  /// Deuteranopia / deuteranomaly — green-weak.
  Deuteranopia,
  /// Protanopia / protanomaly — red-weak.
  Protanopia,
  /// Tritanopia / tritanomaly — blue–yellow.
  Tritanopia,
  /// Achromatopsia — complete color blindness (grayscale output).
  Achromatopsia,
}

impl From<ColorBlindArg> for ColorBlindnessMode {
  fn from(m: ColorBlindArg) -> ColorBlindnessMode {
    match m {
      ColorBlindArg::Deuteranopia => ColorBlindnessMode::Deuteranopia,
      ColorBlindArg::Protanopia => ColorBlindnessMode::Protanopia,
      ColorBlindArg::Tritanopia => ColorBlindnessMode::Tritanopia,
      ColorBlindArg::Achromatopsia => ColorBlindnessMode::Achromatopsia,
    }
  }
}

#[derive(Debug, Parser)]
#[command(
  author,
  version,
  about = "Colorize stdin by hashing its content — the same input always \
           gets the same color.",
  long_about = None,
)]
pub struct CliRaw {
  /// Log level (trace, debug, info, warn, error).
  #[arg(long, env = "LOG_LEVEL", global = true)]
  pub log_level: Option<String>,

  /// Log format (text, json).
  #[arg(long, env = "LOG_FORMAT", global = true)]
  pub log_format: Option<String>,

  /// Apply color to the foreground (default) or background.
  #[arg(short = 'g', long, env = "HASH_COLOR_GROUND", value_name = "GROUND")]
  pub ground: Option<GroundArg>,

  /// Override color-support detection.
  #[arg(long, env = "HASH_COLOR_SUPPORT", value_name = "LEVEL")]
  pub color_support: Option<ColorSupportArg>,

  /// Seed mixed into the hash to shift the color mapping.
  ///
  /// Increment this to resolve color collisions between inputs.
  #[arg(short = 's', long, env = "HASH_COLOR_SEED", value_name = "N")]
  pub seed: Option<u64>,

  /// Saturation for true-color output (0.0–1.0; default 0.7).
  #[arg(long, env = "HASH_COLOR_SATURATION", value_name = "S")]
  pub saturation: Option<f32>,

  /// Lightness for true-color output (0.0–1.0).
  ///
  /// Defaults to 0.60 for foreground and 0.35 for background.
  #[arg(long, env = "HASH_COLOR_LIGHTNESS", value_name = "L")]
  pub lightness: Option<f32>,

  /// Exclude a hue range from color selection (format: START:END in degrees,
  /// e.g. 60:180).  Can be repeated for multiple ranges.
  #[arg(
    long,
    env = "HASH_COLOR_EXCLUDE_HUE",
    value_name = "START:END",
    action = clap::ArgAction::Append,
  )]
  pub exclude_hue: Vec<String>,

  /// Accommodate a color-vision deficiency by excluding its problematic hues.
  ///
  /// This is a shorthand for the corresponding --exclude-hue range(s).
  #[arg(long, env = "HASH_COLOR_COLOR_BLIND", value_name = "MODE")]
  pub color_blind: Option<ColorBlindArg>,

  /// Hash and color each line of stdin independently instead of the whole
  /// input as one block.
  #[arg(short = 'l', long)]
  pub lines: bool,

  /// Hash and colorize VALUE directly instead of reading from stdin.
  ///
  /// Useful when you already have the string in hand and want to avoid a
  /// subprocess: `hash-color --value=foo` instead of `echo foo | hash-color`.
  /// The value is treated as an exact string — no trailing newline is added,
  /// so whitespace sensitivity applies as-is.  --lines is ignored when
  /// --value is present.
  #[arg(long, value_name = "VALUE", env = "HASH_COLOR_VALUE")]
  pub value: Option<String>,

  /// Strip trailing whitespace from the input before hashing.
  ///
  /// By default whitespace is significant: "foo" and "foo\n" hash to different
  /// colors, which lets you detect that two strings differ.  Pass --trim to
  /// get the old behavior where `echo foo | hash-color` and
  /// `printf foo | hash-color` produce the same color.
  ///
  /// In --lines mode line-ending newlines are always stripped (they are
  /// separators, not content), regardless of this flag.
  #[arg(long, env = "HASH_COLOR_TRIM")]
  pub trim: bool,
}

/// Fully validated, ready-to-use configuration.
#[derive(Debug)]
pub struct Config {
  pub log_level: LogLevel,
  pub log_format: LogFormat,
  pub colorizer_options: ColorizerOptions,
  pub lines_mode: bool,
  /// When `true` (the default), the hash key is the raw input bytes.
  /// When `false` (`--trim` was passed), trailing whitespace is stripped first.
  pub whitespace_sensitive: bool,
  /// Direct value passed via `--value`; bypasses stdin when set.
  pub value: Option<String>,
}

impl Config {
  pub fn from_cli(cli: CliRaw) -> Result<Self, ConfigError> {
    let log_level = cli
      .log_level
      .unwrap_or_else(|| "warn".to_string())
      .parse::<LogLevel>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    let log_format = cli
      .log_format
      .unwrap_or_else(|| "text".to_string())
      .parse::<LogFormat>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    let ground: Ground = cli.ground.map(Into::into).unwrap_or_default();
    let color_support: Option<ColorSupport> = cli.color_support.map(Into::into);

    let seed = cli.seed.unwrap_or(0);
    let saturation = cli.saturation.unwrap_or(0.7);
    let lightness = cli.lightness;

    // Parse explicit hue exclusions
    let mut hue_exclusions: Vec<HueExclusion> = cli
      .exclude_hue
      .iter()
      .map(|s| {
        s.parse::<HueExclusion>()
          .map_err(|e| ConfigError::Validation(e.to_string()))
      })
      .collect::<Result<_, _>>()?;

    // Append any color-blindness preset exclusions
    if let Some(mode) = cli.color_blind {
      let preset: ColorBlindnessMode = mode.into();
      hue_exclusions.extend(preset.hue_exclusions());
    }

    let colorizer_options = ColorizerOptions {
      color_support,
      ground,
      seed,
      hue_exclusions,
      saturation,
      lightness,
    };

    Ok(Config {
      log_level,
      log_format,
      colorizer_options,
      lines_mode: cli.lines,
      whitespace_sensitive: !cli.trim,
      value: cli.value,
    })
  }
}
