//! Non-Cryptographic Hashing Utilities

use core::hash::{BuildHasher, Hasher};

/// FNV-1a hasher.
///
/// See https://en.wikipedia.org/wiki/Fowler%E2%80%93Noll%E2%80%93Vo_hash_function
pub struct Fnv1aHasher {
  state: u32,
}

impl Fnv1aHasher {
  const FNV1A_BASIS: u32 = 0x811c9dc5;
  const FNV1A_PRIME: u32 = 0x01000193;

  /// Construct a new Fnv1aHasher initialized with the FNV-1a basis.
  pub fn new() -> Self {
    Fnv1aHasher {
      state: Self::FNV1A_BASIS,
    }
  }
}

impl Hasher for Fnv1aHasher {
  /// Retrieve the current state.
  ///
  /// See https://doc.rust-lang.org/core/hash/trait.Hasher.html#tymethod.finish
  /// The method does not reset the hash.
  fn finish(&self) -> u64 {
    self.state as u64
  }

  /// FNV-1a implementation.
  fn write(&mut self, bytes: &[u8]) {
    for c in bytes {
      self.state ^= *c as u32;
      self.state = self.state.wrapping_mul(Self::FNV1A_PRIME);
    }
  }
}

/// Factory for the FNV-1a hasher.
pub struct BuildFnv1aHasher {}

impl BuildHasher for BuildFnv1aHasher {
  type Hasher = Fnv1aHasher;

  /// Construct a new Fnv1aHasher.
  fn build_hasher(&self) -> Self::Hasher {
    Fnv1aHasher::new()
  }
}
