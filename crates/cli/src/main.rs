//! hash-color — colorize stdin by hashing its content.
//!
//! The same input always produces the same color, making this useful for
//! consistently distinguishing hostnames, user IDs, log tokens, and similar
//! identifiers in terminal output.
//!
//! # LLM Development Guidelines
//! When modifying this code:
//! - Keep configuration logic in config.rs
//! - Keep business logic out of main.rs — use separate modules
//! - Use semantic error types with thiserror — NO anyhow blindly wrapping errors
//! - Add context at each error site explaining WHAT failed and WHY
//! - Keep logging structured and consistent

mod config;
mod logging;

use clap::Parser;
use config::{CliRaw, Config, ConfigError};
use hash_color_lib::HashColorizer;
use logging::init_logging;
use std::io::{self, BufRead, BufReader, Read, Write};
use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
enum ApplicationError {
  #[error("Failed to load configuration: {0}")]
  ConfigurationLoad(#[from] ConfigError),

  #[error("I/O error reading stdin: {0}")]
  StdinRead(#[source] io::Error),

  #[error("I/O error writing stdout: {0}")]
  StdoutWrite(#[source] io::Error),
}

fn main() -> Result<(), ApplicationError> {
  let cli = CliRaw::parse();
  let config = Config::from_cli(cli).map_err(|e| {
    eprintln!("Configuration error: {e}");
    ApplicationError::ConfigurationLoad(e)
  })?;

  init_logging(config.log_level, config.log_format);

  debug!("Starting hash-color");

  run(config)?;

  debug!("hash-color done");
  Ok(())
}

fn run(config: Config) -> Result<(), ApplicationError> {
  let colorizer = HashColorizer::new(config.colorizer_options);
  let stdout = io::stdout();
  let mut out = stdout.lock();

  // --value bypasses stdin entirely.
  if let Some(value) = config.value {
    let key: &[u8] = if config.whitespace_sensitive {
      value.as_bytes()
    } else {
      value
        .trim_end_matches(|c: char| c == '\n' || c == '\r')
        .as_bytes()
    };
    let colored = colorizer.colorize_with_key(key, &value);
    writeln!(out, "{colored}").map_err(ApplicationError::StdoutWrite)?;
    return Ok(());
  }

  if config.lines_mode {
    // Hash and color each line independently.
    let stdin = io::stdin();
    let reader = BufReader::new(stdin.lock());
    for line in reader.lines() {
      let line = line.map_err(ApplicationError::StdinRead)?;
      let colored = colorizer.colorize(&line);
      writeln!(out, "{colored}").map_err(ApplicationError::StdoutWrite)?;
    }
  } else {
    // Read all of stdin, choose a hash key based on the whitespace-sensitivity
    // setting, then write the original bytes with color escapes around them.
    //
    // Default (whitespace-sensitive): hash the raw bytes so that "foo" and
    // "foo\n" get different colors.  This reveals whitespace differences.
    //
    // --trim: strip trailing whitespace before hashing so that
    // `echo foo | hash-color` and `printf foo | hash-color` agree.
    let mut input = String::new();
    io::stdin()
      .lock()
      .read_to_string(&mut input)
      .map_err(ApplicationError::StdinRead)?;

    let key: &[u8] = if config.whitespace_sensitive {
      input.as_bytes()
    } else {
      input
        .trim_end_matches(|c: char| c == '\n' || c == '\r')
        .as_bytes()
    };
    let colored = colorizer.colorize_with_key(key, &input);
    write!(out, "{colored}").map_err(ApplicationError::StdoutWrite)?;
  }

  Ok(())
}
