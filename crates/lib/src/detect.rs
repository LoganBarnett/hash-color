//! Terminal color-support detection from environment variables and terminfo.
//!
//! Respects the [NO_COLOR](https://no-color.org) and
//! [FORCE_COLOR](https://force-color.org) conventions.

use is_terminal::IsTerminal;
use std::str::FromStr;
use terminfo::Database;
use thiserror::Error;

/// The level of color support available in the current terminal.
///
/// Variants are ordered from least to most capable so that `>=` comparisons
/// work as expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ColorSupport {
  /// No color output; emit plain text only.
  None,
  /// 16 standard ANSI colors.
  Ansi16,
  /// 256-color xterm palette.
  Ansi256,
  /// 24-bit true color (16 million colors).
  TrueColor,
}

impl std::fmt::Display for ColorSupport {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ColorSupport::None => write!(f, "none"),
      ColorSupport::Ansi16 => write!(f, "16"),
      ColorSupport::Ansi256 => write!(f, "256"),
      ColorSupport::TrueColor => write!(f, "truecolor"),
    }
  }
}

impl FromStr for ColorSupport {
  type Err = ColorSupportParseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_lowercase().as_str() {
      "none" | "0" | "false" | "off" => Ok(ColorSupport::None),
      "16" | "ansi16" | "ansi" | "basic" => Ok(ColorSupport::Ansi16),
      "256" | "ansi256" | "xterm256" => Ok(ColorSupport::Ansi256),
      "truecolor" | "24bit" | "true" | "full" => Ok(ColorSupport::TrueColor),
      _ => Err(ColorSupportParseError::Unknown(s.to_string())),
    }
  }
}

#[derive(Debug, Error)]
pub enum ColorSupportParseError {
  #[error(
    "Unknown color support level: {0}. \
     Valid values: none, 16, 256, truecolor"
  )]
  Unknown(String),
}

/// Return `true` when the terminfo database entry for `$TERM` declares
/// truecolor support via either the `Tc` extension (tmux convention) or the
/// `setrgbf` capability (official, present in newer databases).
fn terminfo_truecolor() -> bool {
  Database::from_env()
    .map(|db| db.raw("Tc").is_some() || db.raw("setrgbf").is_some())
    .unwrap_or(false)
}

/// Detect the color support level for the current process's stdout.
///
/// Detection order (highest precedence first):
///
/// 1. `NO_COLOR` env var — disables color regardless of other settings.
/// 2. `FORCE_COLOR` env var — forces a specific level (`0`–`3`).
/// 3. `COLORTERM` — `truecolor` or `24bit` signals true-color support.
/// 4. terminfo — `Tc` (tmux truecolor extension) or `setrgbf` capability.
/// 5. `TERM` — suffix hints like `xterm-256color`.
/// 6. stdout is not a TTY — disable color when output is piped.
/// 7. Fallback — assume 16-color support when stdout is a TTY.
pub fn detect_color_support() -> ColorSupport {
  detect_color_support_for(std::io::stdout())
}

/// Detect color support for an arbitrary output stream.
///
/// Useful for testing or for callers that write to a file descriptor other
/// than stdout.
pub fn detect_color_support_for(fd: impl IsTerminal) -> ColorSupport {
  // NO_COLOR wins over everything (https://no-color.org)
  if std::env::var_os("NO_COLOR").is_some() {
    return ColorSupport::None;
  }

  // FORCE_COLOR lets CI/scripts demand a specific level
  if let Ok(v) = std::env::var("FORCE_COLOR") {
    return match v.trim() {
      "0" => ColorSupport::None,
      "1" => ColorSupport::Ansi16,
      "2" => ColorSupport::Ansi256,
      // "3" or any non-zero truthy value → true color
      _ => ColorSupport::TrueColor,
    };
  }

  // COLORTERM signals 24-bit or 256-color support explicitly
  if let Ok(ct) = std::env::var("COLORTERM") {
    match ct.to_lowercase().as_str() {
      "truecolor" | "24bit" => return ColorSupport::TrueColor,
      "256" => return ColorSupport::Ansi256,
      _ => {}
    }
  }

  // Terminfo: Tc is tmux's widely-adopted truecolor extension; setrgbf is
  // the official capability in newer databases.  Either is sufficient.
  if terminfo_truecolor() {
    return ColorSupport::TrueColor;
  }

  // TERM suffix hints
  if let Ok(term) = std::env::var("TERM") {
    match term.to_lowercase().as_str() {
      "dumb" => return ColorSupport::None,
      t if t.contains("truecolor") => return ColorSupport::TrueColor,
      t if t.contains("256color") => return ColorSupport::Ansi256,
      _ => {}
    }
  }

  // Only emit color if the output is a real TTY; piped output gets plain text
  if !fd.is_terminal() {
    return ColorSupport::None;
  }

  ColorSupport::Ansi16
}

#[cfg(test)]
mod tests {
  use super::*;

  struct FakeTty(bool);
  impl IsTerminal for FakeTty {
    fn is_terminal(&self) -> bool {
      self.0
    }
  }

  fn with_env<F: FnOnce()>(vars: &[(&str, &str)], f: F) {
    // Save originals
    let saved: Vec<_> = vars
      .iter()
      .map(|(k, _)| (*k, std::env::var_os(k)))
      .collect();
    for (k, v) in vars {
      std::env::set_var(k, v);
    }
    f();
    // Restore
    for (k, orig) in saved {
      match orig {
        Some(v) => std::env::set_var(k, v),
        None => std::env::remove_var(k),
      }
    }
  }

  #[test]
  fn no_color_disables_all() {
    with_env(&[("NO_COLOR", "1")], || {
      assert_eq!(detect_color_support_for(FakeTty(true)), ColorSupport::None);
    });
  }

  #[test]
  fn force_color_3_gives_truecolor() {
    with_env(&[("FORCE_COLOR", "3")], || {
      assert_eq!(
        detect_color_support_for(FakeTty(false)),
        ColorSupport::TrueColor
      );
    });
  }

  #[test]
  fn non_tty_gives_none() {
    // Clear every env var that influences detection so only the TTY flag matters.
    let clear_vars = ["NO_COLOR", "FORCE_COLOR", "COLORTERM", "TERM"];
    let saved: Vec<_> = clear_vars
      .iter()
      .map(|k| (*k, std::env::var_os(k)))
      .collect();
    for k in &clear_vars {
      std::env::remove_var(k);
    }

    let result = detect_color_support_for(FakeTty(false));

    for (k, v) in saved {
      match v {
        Some(val) => std::env::set_var(k, val),
        None => std::env::remove_var(k),
      }
    }

    assert_eq!(result, ColorSupport::None);
  }
}
