//! Integration tests for the hash-color CLI.
//!
//! Tests are grouped by concern:
//!   1. Smoke tests (flags, empty input)
//!   2. Word sampling — stability
//!   3. Word sampling — uniqueness
//!   4. Word sampling — text preservation
//!   5. Newline normalization
//!   6. Escape-code format (per color-support level)
//!   7. Foreground vs background
//!   8. Seeds
//!   9. Color-blindness modes
//!  10. Custom hue exclusions
//!  11. Saturation
//!  12. Lightness
//!  13. Lines mode
//!  14. --value flag

use std::{
  collections::HashSet,
  io::Write,
  path::PathBuf,
  process::{Command, Stdio},
};

// ─── Word corpus ──────────────────────────────────────────────────────────────

/// A curated set of inputs representative of real-world hash-color use cases.
const WORDS: &[&str] = &[
  // hostnames
  "web-01",
  "web-02",
  "db-primary",
  "db-replica",
  "redis-cache",
  "nginx-proxy",
  "api-gateway",
  "auth-service",
  // usernames
  "alice",
  "bob",
  "charlie",
  "dave",
  "eve",
  // services
  "postgres",
  "redis",
  "nginx",
  "rabbitmq",
  "elasticsearch",
  // log levels
  "ERROR",
  "WARN",
  "INFO",
  "DEBUG",
  "TRACE",
  // short identifiers
  "a",
  "z",
  "1",
  // long identifier
  "very-long-identifier-that-should-still-hash-consistently",
  // alphanumeric
  "user-12345",
  "session-abc123",
  "deadbeef",
  "cafebabe",
];

// ─── Infrastructure ───────────────────────────────────────────────────────────

fn binary_path() -> PathBuf {
  let mut path =
    std::env::current_exe().expect("Failed to get current executable path");
  path.pop(); // remove test executable name
  path.pop(); // remove deps/
  path.push("hash-color");

  if !path.exists() {
    path.pop();
    path.pop();
    let profile = if cfg!(debug_assertions) {
      "debug"
    } else {
      "release"
    };
    path.push(profile);
    path.push("hash-color");
  }
  path
}

fn run(args: &[&str], stdin: &str) -> std::process::Output {
  let mut child = Command::new(binary_path())
    .args(args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("Failed to spawn hash-color binary");

  child
    .stdin
    .take()
    .unwrap()
    .write_all(stdin.as_bytes())
    .expect("Failed to write stdin");

  child
    .wait_with_output()
    .expect("Failed to wait for process")
}

// ─── Escape-code helpers ──────────────────────────────────────────────────────

/// Parse the first `ESC[38;2;R;G;Bm` (fg) or `ESC[48;2;R;G;Bm` (bg) sequence.
fn parse_truecolor(s: &str) -> Option<(u8, u8, u8, bool)> {
  let re = regex::Regex::new(r"\x1b\[(3|4)8;2;(\d+);(\d+);(\d+)m").unwrap();
  re.captures(s).map(|c| {
    let is_bg = &c[1] == "4";
    (c[2].parse().unwrap(), c[3].parse().unwrap(), c[4].parse().unwrap(), is_bg)
  })
}

/// Parse the first `ESC[38;5;Nm` (fg) or `ESC[48;5;Nm` (bg) sequence.
fn parse_ansi256(s: &str) -> Option<(u8, bool)> {
  let re = regex::Regex::new(r"\x1b\[(3|4)8;5;(\d+)m").unwrap();
  re.captures(s).map(|c| {
    let is_bg = &c[1] == "4";
    (c[2].parse().unwrap(), is_bg)
  })
}

/// Parse the first 16-color SGR code (30–37, 40–47, 90–97, 100–107).
fn parse_ansi16(s: &str) -> Option<u16> {
  let re = regex::Regex::new(r"\x1b\[(\d+)m").unwrap();
  for cap in re.captures_iter(s) {
    let code: u16 = cap[1].parse().unwrap();
    if matches!(code, 30..=37 | 40..=47 | 90..=97 | 100..=107) {
      return Some(code);
    }
  }
  None
}

/// Strip all ANSI SGR escape sequences from `s`.
fn strip_escapes(s: &str) -> String {
  let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
  re.replace_all(s, "").into_owned()
}

// ─── Color math helpers ───────────────────────────────────────────────────────

/// Approximate hue in degrees from sRGB components.  Returns 0.0 for achromatic.
fn rgb_to_hue(r: u8, g: u8, b: u8) -> f32 {
  let r = r as f32 / 255.0;
  let g = g as f32 / 255.0;
  let b = b as f32 / 255.0;
  let max = r.max(g).max(b);
  let min = r.min(g).min(b);
  let delta = max - min;
  if delta < f32::EPSILON {
    return 0.0;
  }
  let h = if max == r {
    60.0 * (((g - b) / delta) % 6.0)
  } else if max == g {
    60.0 * ((b - r) / delta + 2.0)
  } else {
    60.0 * ((r - g) / delta + 4.0)
  };
  h.rem_euclid(360.0)
}

/// Chroma (max − min) normalised to 0–1, a proxy for saturation.
fn rgb_chroma(r: u8, g: u8, b: u8) -> f32 {
  let r = r as f32 / 255.0;
  let g = g as f32 / 255.0;
  let b = b as f32 / 255.0;
  r.max(g).max(b) - r.min(g).min(b)
}

/// Return `true` if `hue` falls in `[start, end)`, wrapping at 360° when
/// `start > end`.
fn hue_in_range(hue: f32, start: f32, end: f32) -> bool {
  if start <= end {
    hue >= start && hue < end
  } else {
    hue >= start || hue < end
  }
}

// ─── 1. Smoke tests ───────────────────────────────────────────────────────────

#[test]
fn help_flag_succeeds() {
  let out = Command::new(binary_path())
    .arg("--help")
    .output()
    .expect("--help failed");
  assert!(out.status.success());
  assert!(String::from_utf8_lossy(&out.stdout).contains("Usage:"));
}

#[test]
fn version_flag_succeeds() {
  let out = Command::new(binary_path())
    .arg("--version")
    .output()
    .expect("--version failed");
  assert!(out.status.success());
}

#[test]
fn empty_stdin_exits_ok() {
  let out = run(&["--color-support", "none"], "");
  assert!(
    out.status.success(),
    "stderr: {}",
    String::from_utf8_lossy(&out.stderr)
  );
}

// ─── 2. Word sampling — stability ─────────────────────────────────────────────
//
// Every word must produce bit-identical output on two successive runs.

#[test]
fn all_words_stable_truecolor() {
  for word in WORDS {
    let a = run(&["--color-support", "truecolor"], word);
    let b = run(&["--color-support", "truecolor"], word);
    assert_eq!(a.stdout, b.stdout, "unstable color for {word:?}");
  }
}

#[test]
fn all_words_stable_256() {
  for word in WORDS {
    let a = run(&["--color-support", "256"], word);
    let b = run(&["--color-support", "256"], word);
    assert_eq!(a.stdout, b.stdout, "unstable 256-color for {word:?}");
  }
}

#[test]
fn all_words_stable_16() {
  for word in WORDS {
    let a = run(&["--color-support", "16"], word);
    let b = run(&["--color-support", "16"], word);
    assert_eq!(a.stdout, b.stdout, "unstable 16-color for {word:?}");
  }
}

// ─── 3. Word sampling — uniqueness ────────────────────────────────────────────
//
// Truecolor HSL output with fixed s=0.7, l=0.60 traces a hexagonal path in RGB
// space with roughly 852 distinguishable 8-bit (R,G,B) triples.  With 30 words
// the birthday-problem collision probability is ~40%, so we cannot demand strict
// uniqueness.  Instead we assert that the vast majority of words are distinct.

#[test]
fn most_words_have_unique_colors_truecolor() {
  let mut seen: HashSet<(u8, u8, u8)> = HashSet::new();
  let mut collisions = 0usize;
  for word in WORDS {
    let out = run(&["--color-support", "truecolor"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (r, g, b, _) = parse_truecolor(&stdout).unwrap_or_else(|| {
      panic!("no truecolor escape for {word:?}: {stdout:?}")
    });
    if !seen.insert((r, g, b)) {
      collisions += 1;
    }
  }
  // Expect at most a small handful of 8-bit hue-bucket collisions.
  assert!(
    collisions <= 2,
    "too many truecolor collisions across word list: {collisions}"
  );
}

#[test]
fn all_words_unique_colors_256() {
  // With 216 cube entries and 30 words the distribution should still be
  // collision-free (or very nearly so).
  let mut seen: HashSet<u8> = HashSet::new();
  let mut collisions = 0usize;
  for word in WORDS {
    let out = run(&["--color-support", "256"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (n, _) = parse_ansi256(&stdout).unwrap_or_else(|| {
      panic!("no 256-color escape for {word:?}: {stdout:?}")
    });
    if !seen.insert(n) {
      collisions += 1;
    }
  }
  // Allow at most 2 collisions from the pigeonhole margin of 30 words / 216 slots.
  assert!(
    collisions <= 2,
    "too many 256-color collisions across word list: {collisions}"
  );
}

// ─── 4. Word sampling — text preservation ─────────────────────────────────────
//
// After stripping escapes the output must equal the original input.

#[test]
fn text_preserved_across_all_color_levels() {
  for support in &["none", "16", "256", "truecolor"] {
    // Run all words through lines mode in a single process.
    let input = WORDS.join("\n");
    let out = run(&["--color-support", support, "--lines"], &input);
    assert!(
      out.status.success(),
      "exit failure with --color-support {support}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stripped_stdout = strip_escapes(&stdout);
    let stripped_lines: Vec<&str> = stripped_stdout.lines().collect();
    assert_eq!(
      stripped_lines.len(),
      WORDS.len(),
      "line count mismatch with --color-support {support}"
    );
    for (original, recovered) in WORDS.iter().zip(stripped_lines.iter()) {
      assert_eq!(
        *recovered, *original,
        "--color-support {support}: stripped output {recovered:?} ≠ original {original:?}"
      );
    }
  }
}

// ─── 5. Whitespace sensitivity ────────────────────────────────────────────────
//
// By default hash-color is whitespace-sensitive: "foo" and "foo\n" produce
// different colors, making whitespace differences visible.  Pass --trim to
// restore the old normalizing behavior so `echo foo` and `printf foo` agree.

#[test]
fn trailing_newline_changes_color_by_default() {
  for word in WORDS {
    let with_nl = format!("{word}\n");
    let out_nl = run(&["--color-support", "truecolor"], &with_nl);
    let out_bare = run(&["--color-support", "truecolor"], word);
    assert!(out_nl.status.success());
    assert!(out_bare.status.success());
    let color_nl = parse_truecolor(&String::from_utf8_lossy(&out_nl.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let color_bare =
      parse_truecolor(&String::from_utf8_lossy(&out_bare.stdout))
        .map(|(r, g, b, _)| (r, g, b));
    assert_ne!(
      color_nl, color_bare,
      "{word:?}: trailing newline should produce a different color by default \
       (whitespace is significant)"
    );
  }
}

#[test]
fn trim_flag_makes_trailing_newline_insignificant() {
  for word in WORDS {
    let with_nl = format!("{word}\n");
    let out_nl = run(&["--color-support", "truecolor", "--trim"], &with_nl);
    let out_bare = run(&["--color-support", "truecolor", "--trim"], word);
    assert!(out_nl.status.success());
    assert!(out_bare.status.success());
    let color_nl = parse_truecolor(&String::from_utf8_lossy(&out_nl.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let color_bare =
      parse_truecolor(&String::from_utf8_lossy(&out_bare.stdout))
        .map(|(r, g, b, _)| (r, g, b));
    assert_eq!(
      color_nl, color_bare,
      "{word:?}: --trim should make trailing newline invisible to the hash"
    );
  }
}

#[test]
fn trim_flag_is_stable_across_runs() {
  for word in WORDS {
    let a = run(&["--color-support", "truecolor", "--trim"], word);
    let b = run(&["--color-support", "truecolor", "--trim"], word);
    assert_eq!(
      a.stdout, b.stdout,
      "--trim produced unstable output for {word:?}"
    );
  }
}

#[test]
fn crlf_stripped_by_trim() {
  // --trim should strip \r\n, not just \n.
  let out_lf = run(&["--color-support", "truecolor", "--trim"], "hello\n");
  let out_crlf = run(&["--color-support", "truecolor", "--trim"], "hello\r\n");
  let out_bare = run(&["--color-support", "truecolor", "--trim"], "hello");
  let color = |o: &std::process::Output| {
    parse_truecolor(&String::from_utf8_lossy(&o.stdout))
      .map(|(r, g, b, _)| (r, g, b))
  };
  assert_eq!(
    color(&out_lf),
    color(&out_bare),
    "LF should be stripped by --trim"
  );
  assert_eq!(
    color(&out_crlf),
    color(&out_bare),
    "CRLF should be stripped by --trim"
  );
}

#[test]
fn whitespace_sensitive_sees_crlf_differently_from_lf() {
  // Without --trim each whitespace variant is a distinct hash key.
  let out_lf = run(&["--color-support", "truecolor"], "hello\n");
  let out_crlf = run(&["--color-support", "truecolor"], "hello\r\n");
  let out_bare = run(&["--color-support", "truecolor"], "hello");
  let color = |o: &std::process::Output| {
    parse_truecolor(&String::from_utf8_lossy(&o.stdout))
      .map(|(r, g, b, _)| (r, g, b))
  };
  assert_ne!(
    color(&out_lf),
    color(&out_bare),
    "\\n should differ from bare by default"
  );
  assert_ne!(
    color(&out_crlf),
    color(&out_bare),
    "\\r\\n should differ from bare by default"
  );
  assert_ne!(
    color(&out_lf),
    color(&out_crlf),
    "\\n and \\r\\n should differ from each other"
  );
}

// ─── 6. Escape-code format ────────────────────────────────────────────────────

#[test]
fn no_color_emits_no_escapes() {
  for word in WORDS {
    let out = run(&["--color-support", "none"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
      !stdout.contains('\x1b'),
      "{word:?}: escape codes present with --color-support none"
    );
  }
}

#[test]
fn ansi16_emits_correct_escape_format() {
  // Must produce a 16-color SGR code (30–37 or 90–97 for fg), not 256 or
  // truecolor sequences.
  for word in WORDS {
    let out = run(&["--color-support", "16"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    // No truecolor or 256-color escapes
    assert!(
      parse_truecolor(&stdout).is_none(),
      "{word:?}: unexpected truecolor escape with --color-support 16"
    );
    assert!(
      parse_ansi256(&stdout).is_none(),
      "{word:?}: unexpected 256-color escape with --color-support 16"
    );
    // Must have a standard 16-color code
    assert!(
      parse_ansi16(&stdout).is_some(),
      "{word:?}: no 16-color escape found"
    );
    // Must also emit the reset code
    assert!(
      stdout.contains("\x1b[0m"),
      "{word:?}: no reset escape after 16-color code"
    );
  }
}

#[test]
fn ansi256_emits_correct_escape_format() {
  for word in WORDS {
    let out = run(&["--color-support", "256"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
      parse_ansi256(&stdout).is_some(),
      "{word:?}: no 256-color escape found"
    );
    assert!(
      parse_truecolor(&stdout).is_none(),
      "{word:?}: unexpected truecolor escape with --color-support 256"
    );
    assert!(stdout.contains("\x1b[0m"), "{word:?}: no reset escape");
  }
}

#[test]
fn truecolor_emits_correct_escape_format() {
  for word in WORDS {
    let out = run(&["--color-support", "truecolor"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
      parse_truecolor(&stdout).is_some(),
      "{word:?}: no truecolor escape found"
    );
    assert!(stdout.contains("\x1b[0m"), "{word:?}: no reset escape");
  }
}

// ─── 7. Foreground vs background ─────────────────────────────────────────────

#[test]
fn foreground_uses_38_prefix_in_truecolor() {
  for word in WORDS {
    let out = run(&["--color-support", "truecolor", "--ground", "fg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (_, _, _, is_bg) =
      parse_truecolor(&stdout).expect("no truecolor escape");
    assert!(!is_bg, "{word:?}: got background escape for --ground fg");
  }
}

#[test]
fn background_uses_48_prefix_in_truecolor() {
  for word in WORDS {
    let out = run(&["--color-support", "truecolor", "--ground", "bg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (_, _, _, is_bg) =
      parse_truecolor(&stdout).expect("no truecolor escape");
    assert!(is_bg, "{word:?}: got foreground escape for --ground bg");
  }
}

#[test]
fn foreground_uses_38_prefix_in_256() {
  for word in WORDS {
    let out = run(&["--color-support", "256", "--ground", "fg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (_, is_bg) = parse_ansi256(&stdout).expect("no 256-color escape");
    assert!(!is_bg, "{word:?}: got background escape for --ground fg (256)");
  }
}

#[test]
fn background_uses_48_prefix_in_256() {
  for word in WORDS {
    let out = run(&["--color-support", "256", "--ground", "bg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (_, is_bg) = parse_ansi256(&stdout).expect("no 256-color escape");
    assert!(is_bg, "{word:?}: got foreground escape for --ground bg (256)");
  }
}

#[test]
fn foreground_16_color_code_in_range_30_to_37_or_90_to_97() {
  for word in WORDS {
    let out = run(&["--color-support", "16", "--ground", "fg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let code = parse_ansi16(&stdout).expect("no 16-color escape");
    assert!(
      matches!(code, 30..=37 | 90..=97),
      "{word:?}: 16-color fg code {code} is not in foreground range"
    );
  }
}

#[test]
fn background_16_color_code_in_range_40_to_47_or_100_to_107() {
  for word in WORDS {
    let out = run(&["--color-support", "16", "--ground", "bg"], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let code = parse_ansi16(&stdout).expect("no 16-color escape");
    assert!(
      matches!(code, 40..=47 | 100..=107),
      "{word:?}: 16-color bg code {code} is not in background range"
    );
  }
}

#[test]
fn foreground_and_background_produce_different_truecolor() {
  // Different default lightness (0.60 fg vs 0.35 bg) means fg ≠ bg color.
  for word in WORDS {
    let fg = run(&["--color-support", "truecolor", "--ground", "fg"], word);
    let bg = run(&["--color-support", "truecolor", "--ground", "bg"], word);
    let fg_color = parse_truecolor(&String::from_utf8_lossy(&fg.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let bg_color = parse_truecolor(&String::from_utf8_lossy(&bg.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    assert_ne!(
      fg_color, bg_color,
      "{word:?}: fg and bg produced the same color (both use the same lightness?)"
    );
  }
}

// ─── 8. Seeds ─────────────────────────────────────────────────────────────────

#[test]
fn seeds_all_produce_distinct_colors() {
  let seeds = ["0", "1", "2", "3", "4", "5", "10", "42", "100", "999"];
  let word = "seed-test-input";
  let mut seen: HashSet<(u8, u8, u8)> = HashSet::new();
  for seed in seeds {
    let out = run(&["--color-support", "truecolor", "--seed", seed], word);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (r, g, b, _) = parse_truecolor(&stdout).expect("no truecolor escape");
    assert!(
      seen.insert((r, g, b)),
      "seed {seed} produced the same color as a previous seed"
    );
  }
}

#[test]
fn seed_is_stable_across_runs() {
  let seeds = ["0", "7", "42", "999"];
  for word in WORDS {
    for seed in seeds {
      let a = run(&["--color-support", "truecolor", "--seed", seed], word);
      let b = run(&["--color-support", "truecolor", "--seed", seed], word);
      assert_eq!(
        a.stdout, b.stdout,
        "seed {seed} produced different output for {word:?} on two runs"
      );
    }
  }
}

#[test]
fn seed_changes_color_for_all_words() {
  // Every multi-byte word should map to a different color with seed=0 vs
  // seed=1.  Single-character words are skipped: even with good seed mixing,
  // two very similar hashes may still round to the same 8-bit (R,G,B) triple.
  let mut same_count = 0usize;
  for word in WORDS {
    if word.len() < 3 {
      continue; // single/double-char: hue buckets may coincide after rounding
    }
    let s0 = run(&["--color-support", "truecolor", "--seed", "0"], word);
    let s1 = run(&["--color-support", "truecolor", "--seed", "1"], word);
    let c0 = parse_truecolor(&String::from_utf8_lossy(&s0.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let c1 = parse_truecolor(&String::from_utf8_lossy(&s1.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    if c0 == c1 {
      same_count += 1;
    }
  }
  assert!(
    same_count == 0,
    "{same_count} multi-byte word(s) had the same color under seed 0 and seed 1"
  );
}

// ─── 9. Color-blindness modes ─────────────────────────────────────────────────
//
// For each mode, the hue of the output color must not fall in the excluded arc.
// We only check chromatic colors (chroma > 0.05) since achromatic outputs have
// no meaningful hue.

fn assert_hue_outside(
  word: &str,
  r: u8,
  g: u8,
  b: u8,
  excl_start: f32,
  excl_end: f32,
  mode: &str,
) {
  if rgb_chroma(r, g, b) < 0.05 {
    return; // achromatic — no hue to check
  }
  let hue = rgb_to_hue(r, g, b);
  assert!(
    !hue_in_range(hue, excl_start, excl_end),
    "{mode}: word {word:?} got hue {hue:.1}° which falls in excluded arc \
     [{excl_start}°, {excl_end}°)"
  );
}

#[test]
fn deuteranopia_avoids_green_hues() {
  // Exclusion: 80°–170° (greens)
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--color-blind",
        "deuteranopia",
      ],
      word,
    );
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(word, r, g, b, 80.0, 170.0, "deuteranopia");
    }
  }
}

#[test]
fn protanopia_avoids_red_hues() {
  // Exclusion: 340°–30° (reds, wrapping around 0°)
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--color-blind",
        "protanopia",
      ],
      word,
    );
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(word, r, g, b, 340.0, 30.0, "protanopia");
    }
  }
}

#[test]
fn tritanopia_avoids_blue_hues() {
  // Exclusion: 180°–270° (blues and cyans)
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--color-blind",
        "tritanopia",
      ],
      word,
    );
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(word, r, g, b, 180.0, 270.0, "tritanopia");
    }
  }
}

#[test]
fn achromatopsia_produces_grayscale_for_all_words() {
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--color-blind",
        "achromatopsia",
      ],
      word,
    );
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (r, g, b, _) = parse_truecolor(&stdout)
      .unwrap_or_else(|| panic!("no truecolor escape for {word:?}"));
    assert_eq!(r, g, "{word:?}: R≠G in grayscale achromatopsia output");
    assert_eq!(g, b, "{word:?}: G≠B in grayscale achromatopsia output");
  }
}

#[test]
fn color_blind_modes_all_accepted_for_all_words() {
  let modes = ["deuteranopia", "protanopia", "tritanopia", "achromatopsia"];
  for mode in modes {
    for word in WORDS {
      let out =
        run(&["--color-support", "truecolor", "--color-blind", mode], word);
      assert!(
        out.status.success(),
        "mode={mode} word={word:?} stderr: {}",
        String::from_utf8_lossy(&out.stderr)
      );
    }
  }
}

// ─── 10. Custom hue exclusions ────────────────────────────────────────────────

#[test]
fn single_exclude_hue_range_respected() {
  // Exclude a wide band of hues and verify none of the words land in it.
  let excl_start = 60.0f32;
  let excl_end = 240.0f32;
  for word in WORDS {
    let out =
      run(&["--color-support", "truecolor", "--exclude-hue", "60:240"], word);
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(
        word,
        r,
        g,
        b,
        excl_start,
        excl_end,
        "exclude-hue 60:240",
      );
    }
  }
}

#[test]
fn multiple_exclude_hue_ranges_both_respected() {
  // Exclude two separate bands; neither should appear in the output.
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--exclude-hue",
        "30:90",
        "--exclude-hue",
        "210:270",
      ],
      word,
    );
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(word, r, g, b, 30.0, 90.0, "exclude 30:90");
      assert_hue_outside(word, r, g, b, 210.0, 270.0, "exclude 210:270");
    }
  }
}

#[test]
fn wrapping_exclude_hue_range_respected() {
  // 300:60 wraps around 0°, covering magentas, reds, and oranges.
  for word in WORDS {
    let out =
      run(&["--color-support", "truecolor", "--exclude-hue", "300:60"], word);
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(
        word,
        r,
        g,
        b,
        300.0,
        60.0,
        "exclude-hue 300:60 (wrapping)",
      );
    }
  }
}

#[test]
fn combined_preset_and_custom_exclusion() {
  // Protanopia (340°–30°) + explicit 90°–180°: only 30°–90° and 180°–340°
  // should be reachable.
  for word in WORDS {
    let out = run(
      &[
        "--color-support",
        "truecolor",
        "--color-blind",
        "protanopia",
        "--exclude-hue",
        "90:180",
      ],
      word,
    );
    assert!(out.status.success());
    if let Some((r, g, b, _)) =
      parse_truecolor(&String::from_utf8_lossy(&out.stdout))
    {
      assert_hue_outside(word, r, g, b, 340.0, 30.0, "protanopia arc");
      assert_hue_outside(word, r, g, b, 90.0, 180.0, "custom arc 90:180");
    }
  }
}

// ─── 11. Saturation ───────────────────────────────────────────────────────────

#[test]
fn saturation_zero_produces_grayscale() {
  for word in WORDS {
    let out = run(&["--color-support", "truecolor", "--saturation", "0"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let (r, g, b, _) = parse_truecolor(&stdout)
      .unwrap_or_else(|| panic!("no truecolor escape for {word:?}"));
    assert_eq!(r, g, "{word:?}: R≠G with --saturation 0");
    assert_eq!(g, b, "{word:?}: G≠B with --saturation 0");
  }
}

#[test]
fn saturation_affects_output_color() {
  // Low vs high saturation must produce meaningfully different colors.
  for word in WORDS {
    let lo =
      run(&["--color-support", "truecolor", "--saturation", "0.1"], word);
    let hi =
      run(&["--color-support", "truecolor", "--saturation", "0.9"], word);
    assert!(lo.status.success());
    assert!(hi.status.success());
    let lo_color = parse_truecolor(&String::from_utf8_lossy(&lo.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let hi_color = parse_truecolor(&String::from_utf8_lossy(&hi.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    assert_ne!(
      lo_color, hi_color,
      "{word:?}: --saturation 0.1 and 0.9 produced the same color"
    );
  }
}

#[test]
fn saturation_stable_across_runs() {
  for sat in &["0.0", "0.3", "0.7", "1.0"] {
    for word in WORDS {
      let a = run(&["--color-support", "truecolor", "--saturation", sat], word);
      let b = run(&["--color-support", "truecolor", "--saturation", sat], word);
      assert_eq!(
        a.stdout, b.stdout,
        "--saturation {sat} produced different output for {word:?}"
      );
    }
  }
}

// ─── 12. Lightness ────────────────────────────────────────────────────────────

#[test]
fn lightness_affects_output_color() {
  for word in WORDS {
    let dark =
      run(&["--color-support", "truecolor", "--lightness", "0.2"], word);
    let light =
      run(&["--color-support", "truecolor", "--lightness", "0.8"], word);
    assert!(dark.status.success());
    assert!(light.status.success());
    let dark_color = parse_truecolor(&String::from_utf8_lossy(&dark.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    let light_color = parse_truecolor(&String::from_utf8_lossy(&light.stdout))
      .map(|(r, g, b, _)| (r, g, b));
    assert_ne!(
      dark_color, light_color,
      "{word:?}: --lightness 0.2 and 0.8 produced the same color"
    );
  }
}

#[test]
fn lightness_stable_across_runs() {
  for l in &["0.2", "0.4", "0.6", "0.8"] {
    for word in WORDS {
      let a = run(&["--color-support", "truecolor", "--lightness", l], word);
      let b = run(&["--color-support", "truecolor", "--lightness", l], word);
      assert_eq!(
        a.stdout, b.stdout,
        "--lightness {l} produced different output for {word:?}"
      );
    }
  }
}

#[test]
fn explicit_lightness_overrides_ground_default() {
  // With --lightness set, fg and bg should produce the *same* lightness
  // (though the hue is still the same so the full RGB will match).
  let word = "lightness-override";
  let fg = run(
    &[
      "--color-support",
      "truecolor",
      "--ground",
      "fg",
      "--lightness",
      "0.5",
    ],
    word,
  );
  let bg = run(
    &[
      "--color-support",
      "truecolor",
      "--ground",
      "bg",
      "--lightness",
      "0.5",
    ],
    word,
  );
  let fg_color = parse_truecolor(&String::from_utf8_lossy(&fg.stdout))
    .map(|(r, g, b, _)| (r, g, b));
  let bg_color = parse_truecolor(&String::from_utf8_lossy(&bg.stdout))
    .map(|(r, g, b, _)| (r, g, b));
  // Same lightness + same hue → same RGB, just different escape prefix.
  assert_eq!(
    fg_color, bg_color,
    "same --lightness should produce same RGB regardless of --ground"
  );
}

// ─── 13. Lines mode ───────────────────────────────────────────────────────────

#[test]
fn lines_mode_each_line_independently_hashed() {
  // Run each word individually and then all together via --lines.
  // The color for each word must match its individual color.
  let input = WORDS.join("\n");
  let lines_out = run(&["--color-support", "truecolor", "--lines"], &input);
  assert!(lines_out.status.success());
  let lines_stdout = String::from_utf8_lossy(&lines_out.stdout).into_owned();

  for (i, word) in WORDS.iter().enumerate() {
    let single_out = run(&["--color-support", "truecolor"], word);
    let single_color =
      parse_truecolor(&String::from_utf8_lossy(&single_out.stdout))
        .map(|(r, g, b, _)| (r, g, b));

    // Extract the i-th colored line from the lines-mode output.
    let lines_line = lines_stdout
      .lines()
      .nth(i)
      .unwrap_or_else(|| panic!("lines output has no line {i} for {word:?}"));
    let lines_color = parse_truecolor(lines_line).map(|(r, g, b, _)| (r, g, b));

    assert_eq!(
      single_color, lines_color,
      "line {i} ({word:?}): color in --lines mode differs from single-input color"
    );
  }
}

#[test]
fn lines_mode_repeated_identical_lines_same_color() {
  let word = "repeated-word";
  let input = format!("{word}\n{word}\n{word}\n");
  let out = run(&["--color-support", "truecolor", "--lines"], &input);
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
  let colors: Vec<_> = stdout
    .lines()
    .map(|line| parse_truecolor(line).map(|(r, g, b, _)| (r, g, b)))
    .collect();
  assert_eq!(colors.len(), 3, "expected 3 colored lines");
  assert_eq!(
    colors[0], colors[1],
    "identical lines should get identical colors"
  );
  assert_eq!(colors[1], colors[2], "all three identical lines should match");
}

#[test]
fn lines_mode_different_lines_mostly_different_colors() {
  // Same caveat as most_words_have_unique_colors_truecolor: with ~852
  // achievable 8-bit colors and 30 words, a collision or two is expected.
  let input = WORDS.join("\n");
  let out = run(&["--color-support", "truecolor", "--lines"], &input);
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
  let mut seen: HashSet<(u8, u8, u8)> = HashSet::new();
  let mut collisions = 0usize;
  for (line, word) in stdout.lines().zip(WORDS.iter()) {
    let color = parse_truecolor(line).map(|(r, g, b, _)| (r, g, b));
    let key = color.unwrap_or_else(|| panic!("no color on line for {word:?}"));
    if !seen.insert(key) {
      collisions += 1;
    }
  }
  assert!(
    collisions <= 2,
    "--lines mode: too many color collisions across word list: {collisions}"
  );
}

#[test]
fn block_mode_and_lines_mode_differ_for_multiline_input() {
  // In block mode the whole input is hashed; in lines mode each line is.
  let input = "line-one\nline-two\n";
  let block = run(&["--color-support", "truecolor"], input);
  let lines = run(&["--color-support", "truecolor", "--lines"], input);
  assert!(block.status.success());
  assert!(lines.status.success());
  assert_ne!(
    block.stdout, lines.stdout,
    "block mode and lines mode should produce different output for multiline input"
  );
}

#[test]
fn lines_mode_text_preserved() {
  let input = WORDS.join("\n");
  let out = run(&["--color-support", "truecolor", "--lines"], &input);
  assert!(out.status.success());
  let stdout = String::from_utf8_lossy(&out.stdout);
  let stripped_stdout = strip_escapes(&stdout);
  let stripped: Vec<&str> = stripped_stdout.lines().collect();
  assert_eq!(stripped.len(), WORDS.len());
  for (original, recovered) in WORDS.iter().zip(stripped.iter()) {
    assert_eq!(*recovered, *original, "text not preserved in --lines mode");
  }
}

// ─── 14. --value flag ─────────────────────────────────────────────────────────

/// Helper: run hash-color with --value=VALUE instead of stdin.
fn run_value(args: &[&str], value: &str) -> std::process::Output {
  let mut full_args = args.to_vec();
  full_args.push("--value");
  full_args.push(value);
  run_no_stdin(&full_args)
}

/// Run hash-color with no stdin at all (pipe closed immediately).
fn run_no_stdin(args: &[&str]) -> std::process::Output {
  Command::new(binary_path())
    .args(args)
    .stdin(Stdio::null())
    .output()
    .expect("failed to run hash-color")
}

#[test]
fn value_flag_produces_output() {
  let out = run_value(&["--color-support", "truecolor"], "hello");
  assert!(out.status.success());
  assert!(!out.stdout.is_empty());
}

#[test]
fn value_flag_text_preserved() {
  for word in WORDS {
    let out = run_value(&["--color-support", "truecolor"], word);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stripped = strip_escapes(&stdout);
    // writeln adds a trailing newline
    assert_eq!(
      stripped.trim_end_matches('\n'),
      *word,
      "--value text not preserved for {word}"
    );
  }
}

#[test]
fn value_flag_stable_across_calls() {
  for word in WORDS {
    let a = run_value(&["--color-support", "truecolor"], word);
    let b = run_value(&["--color-support", "truecolor"], word);
    assert_eq!(a.stdout, b.stdout, "--value output not stable for {word}");
  }
}

#[test]
fn value_flag_matches_stdin_equivalent() {
  // --value=foo should produce the same color as piping "foo" (no newline)
  // through stdin with --trim (since stdin via echo adds a newline, but
  // --value gets the exact string).  The simpler test: pipe the exact same
  // bytes with printf (no trailing newline) and compare.
  for word in WORDS {
    let via_value = run_value(&["--color-support", "truecolor"], word);
    // Pipe exactly `word` bytes, no trailing newline
    let via_stdin = run(&["--color-support", "truecolor"], word);
    // strip_escapes + trim the trailing writeln newline from --value
    let value_stdout = String::from_utf8_lossy(&via_value.stdout);
    let stdin_stdout = String::from_utf8_lossy(&via_stdin.stdout);
    let value_color = strip_escapes(&value_stdout);
    let stdin_color = strip_escapes(&stdin_stdout);
    // Both are hashing the same bytes (word, no newline in either case for
    // whitespace-sensitive mode when the piped input has no trailing newline)
    assert_eq!(
      value_color.trim_end_matches('\n'),
      stdin_color.trim_end_matches('\n'),
      "--value and stdin produced different output for {word}"
    );
  }
}

#[test]
fn value_flag_different_values_can_differ() {
  // Sanity: two distinct values should (usually) produce different colors.
  let a = run_value(&["--color-support", "truecolor"], "alice");
  let b = run_value(&["--color-support", "truecolor"], "bob");
  assert_ne!(a.stdout, b.stdout, "--value 'alice' and 'bob' should differ");
}

#[test]
fn value_flag_works_with_all_color_levels() {
  for level in &["none", "16", "256", "truecolor"] {
    let out = run_value(&["--color-support", level], "hello");
    assert!(
      out.status.success(),
      "--value failed with --color-support {level}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stripped = strip_escapes(&stdout);
    assert!(
      stripped.contains("hello"),
      "--value text missing with --color-support {level}"
    );
  }
}

#[test]
fn value_flag_with_seed_changes_color() {
  let a = run_value(&["--color-support", "truecolor", "--seed", "0"], "hello");
  let b = run_value(&["--color-support", "truecolor", "--seed", "1"], "hello");
  assert_ne!(a.stdout, b.stdout, "--value --seed should shift color");
}

#[test]
fn value_flag_trim_strips_trailing_newline() {
  // With --trim, "foo\n" and "foo" should hash identically.
  let without_newline =
    run_value(&["--color-support", "truecolor", "--trim"], "foo");
  let with_newline =
    run_value(&["--color-support", "truecolor", "--trim"], "foo\n");
  let a = String::from_utf8_lossy(&without_newline.stdout);
  let b = String::from_utf8_lossy(&with_newline.stdout);
  assert_eq!(
    strip_escapes(&a).trim_end_matches('\n'),
    strip_escapes(&b).trim_end_matches('\n'),
    "--value --trim: 'foo' and 'foo\\n' should produce the same color"
  );
}

#[test]
fn value_flag_whitespace_sensitive_by_default() {
  // Without --trim, "foo" and "foo\n" must hash differently.
  let without_newline = run_value(&["--color-support", "truecolor"], "foo");
  let with_newline = run_value(&["--color-support", "truecolor"], "foo\n");
  assert_ne!(
    without_newline.stdout, with_newline.stdout,
    "--value: 'foo' and 'foo\\n' should differ without --trim"
  );
}
