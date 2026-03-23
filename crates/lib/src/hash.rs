//! Hashing for stable, deterministic color assignment.
//!
//! The core algorithm is FNV-1a with a SplitMix64 finalizer applied after the
//! FNV accumulation pass.  The finalizer dramatically improves avalanche: inputs
//! that differ by only one byte (e.g. `"web-01"` vs `"web-02"`) produce
//! completely different hash values rather than nearly identical ones.
//!
//! Seeds are incorporated via Fibonacci hashing before the FNV pass, ensuring
//! that adjacent seeds (e.g. 0 and 1) yield well-separated initial states.
//! seed = 0 is the identity perturbation, so `fnv1a_64(x) == fnv1a_64_seeded(x, 0)`.

/// SplitMix64 finalizer — excellent avalanche, reversible, branch-free.
///
/// Applying this to the raw FNV-1a output ensures that similar inputs map to
/// uncorrelated hash values.
fn finalize(mut h: u64) -> u64 {
  h ^= h >> 30;
  h = h.wrapping_mul(0xbf58476d1ce4e5b9);
  h ^= h >> 27;
  h = h.wrapping_mul(0x94d049bb133111eb);
  h ^= h >> 31;
  h
}

/// Internal FNV-1a accumulation without finalizer.
fn fnv1a_raw(data: &[u8], initial: u64) -> u64 {
  const PRIME: u64 = 1099511628211;
  let mut hash = initial;
  for &byte in data {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(PRIME);
  }
  hash
}

/// Compute a deterministic 64-bit hash of `data`.
///
/// Uses FNV-1a accumulation followed by a SplitMix64 finalizer for good
/// avalanche properties.  The same byte sequence always produces the same
/// value across platforms and Rust versions.
pub fn fnv1a_64(data: &[u8]) -> u64 {
  fnv1a_64_seeded(data, 0)
}

/// Compute a deterministic 64-bit hash with an explicit `seed`.
///
/// The seed shifts which color is assigned to each input, allowing users to
/// resolve unwanted color collisions.  Adjacent seeds (e.g. 0 and 1) produce
/// well-separated hash values thanks to Fibonacci hashing of the seed before
/// it enters the FNV accumulation.
///
/// `seed = 0` is equivalent to calling [`fnv1a_64`].
pub fn fnv1a_64_seeded(data: &[u8], seed: u64) -> u64 {
  const FNV_OFFSET: u64 = 14695981039346656037;
  // Spread the seed through all 64 bits via the Fibonacci/golden-ratio
  // constant.  Multiplying by 0 leaves the offset unchanged (seed=0 identity).
  let seed_spread = seed.wrapping_mul(0x9e3779b97f4a7c15);
  let initial = FNV_OFFSET.wrapping_add(seed_spread);
  finalize(fnv1a_raw(data, initial))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn same_input_same_hash() {
    assert_eq!(fnv1a_64(b"hello"), fnv1a_64(b"hello"));
  }

  #[test]
  fn different_inputs_different_hashes() {
    assert_ne!(fnv1a_64(b"hello"), fnv1a_64(b"world"));
  }

  #[test]
  fn similar_inputs_different_hashes() {
    // Avalanche: single-byte difference must produce a different hash.
    assert_ne!(fnv1a_64(b"web-01"), fnv1a_64(b"web-02"));
    assert_ne!(fnv1a_64(b"web-01"), fnv1a_64(b"web-03"));
    assert_ne!(fnv1a_64(b"alice"), fnv1a_64(b"alicf"));
  }

  #[test]
  fn seed_changes_hash() {
    assert_ne!(fnv1a_64_seeded(b"hello", 0), fnv1a_64_seeded(b"hello", 1));
  }

  #[test]
  fn adjacent_seeds_produce_different_hashes() {
    // Fibonacci seed mixing must spread adjacent seeds across the hash space.
    let h0 = fnv1a_64_seeded(b"a", 0);
    let h1 = fnv1a_64_seeded(b"a", 1);
    let h2 = fnv1a_64_seeded(b"a", 2);
    assert_ne!(h0, h1);
    assert_ne!(h1, h2);
    assert_ne!(h0, h2);
  }

  #[test]
  fn seed_zero_matches_unseeded() {
    assert_eq!(fnv1a_64(b"hello"), fnv1a_64_seeded(b"hello", 0));
  }

  #[test]
  fn hash_is_deterministic_across_calls() {
    for _ in 0..100 {
      assert_eq!(fnv1a_64(b"consistent"), fnv1a_64(b"consistent"));
    }
  }
}
