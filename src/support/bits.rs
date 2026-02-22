//! Bit manipulation utilities.
//!
//! http://aggregate.org/MAGIC/
//! http://graphics.stanford.edu/~seander/bithacks.html

#[cfg(feature = "module_tests")]
mod tests;

pub use crate::arch::bits::*;

use core::cmp;
use crate::debug_print;
#[cfg(feature = "module_tests")]
use crate::test;

/// The number of bits in a machine word.
pub const WORD_BITS: usize = usize::BITS as usize;

/// The machine word bit size shift.
pub const WORD_BIT_SHIFT: usize = floor_log2(WORD_BITS);

/// The machine word bit mask.
pub const WORD_BIT_MASK: usize = WORD_BITS - 1;

/// The number of bytes in a machine word.
pub const WORD_BYTES: usize = WORD_BITS >> 3;

/// The machine word byte size shift.
pub const WORD_SHIFT: usize = floor_log2(WORD_BYTES);

/// The machine word byte mask.
pub const WORD_MASK: usize = WORD_BYTES - 1;

/// Aligns an address with the start of the boundary.
///
/// # Parameters
///
/// * `addr` - The address to align.
/// * `boundary` - The alignment boundary.
///
/// # Assumptions
///
/// `boundary` is assumed to be greater than 0. If 0, the subtraction will
/// assert.
///
/// `boundary` is assumed to be a power of 2.
///
/// # Returns
///
/// The aligned address.
pub const fn align_down(addr: usize, boundary: usize) -> usize {
  addr & !(boundary - 1)
}

/// Aligns an address with the start of the next boundary.
///
/// # Parameters
///
/// * `addr` - The address to align.
/// * `boundary` - The alignment boundary.
///
/// # Assumptions
///
/// `boundary` is assumed to be greater than 0. If 0, the subtraction will
/// assert.
///
/// `boundary` is assumed to be a power of 2.
///
/// # Returns
///
/// The aligned address.
pub const fn align_up(addr: usize, boundary: usize) -> usize {
  let b = boundary - 1;
  (addr + b) & !b
}

/// Check if an address is aligned with a boundary.
///
/// # Parameters
///
/// * `addr` - The address to check.
/// * `boundary` - The alignment boundary.
///
/// # Assumptions
///
/// `boundary` is assumed to be greater than 0. If 0, the subtraction will
/// assert.
///
/// `boundary` is assumed to be a power of 2.
///
/// # Returns
///
/// True if the address is aligned, false otherwise.
pub const fn is_aligned(addr: usize, boundary: usize) -> bool {
  addr & !(boundary - 1) == addr
}

/// Fast check if a number is a power of 2.
///
/// # Parameters
///
/// * `n` - The number to check.
///
/// # Returns
///
/// True if the number is a power of 2, false otherwise.
pub const fn is_power_of_2(n: usize) -> bool {
  // The check against 0 ensures 0 is not reported as a power of 2 and prevents
  // the subtraction from asserting.
  (n != 0) && ((n & (n - 1)) == 0)
}

/// Fast least-significant bit mask.
///
/// # Parameters
///
/// `n` - The number to mask off.
///
/// # Returns
///
/// A mask for the least-significant bit in `n`.
pub const fn least_significant_bit(n: usize) -> usize {
  n & ((!n).wrapping_add(1))
}

/// Simple XOR checksum of a list of words.
///
/// # Parameters
///
/// * `words` - A slice of usize words to sum.
///
/// # Returns
///
/// The words XOR'd with a random, constant seed.
pub fn xor_checksum(words: &[usize]) -> usize {
  let mut sum = CHECKSUM_SEED;

  for w in words {
    sum ^= w;
  }

  sum
}

/// A multiword bitmap that can store up to `MAP_WORDS << bits::WORD_BIT_SHIFT`
/// bits.
#[derive(Copy, Clone)]
pub struct Bitmap<const MAP_WORDS: usize> {
  bitmap: [usize; MAP_WORDS],
  bits: usize,
}

impl<const MAP_WORDS: usize> Bitmap<MAP_WORDS> {
  const BITMAP_INITIALIZER: [usize; MAP_WORDS] = [0; MAP_WORDS];

  /// Construct a new bitmap.
  ///
  /// # Parameters
  ///
  /// * `bits` - The maximum number of bits.
  ///
  /// # Description
  ///
  /// The number of bits the map can store will be capped to the size of the
  /// buffer.
  pub fn new(bits: usize) -> Self {
    let max_bits = MAP_WORDS << WORD_BIT_SHIFT;

    Self {
      bitmap: Self::BITMAP_INITIALIZER,
      bits: cmp::min(bits, max_bits),
    }
  }

  /// Get the number of bits available in the map.
  pub fn len(&self) -> usize {
    self.bits
  }

  /// Set a bit in the mask.
  ///
  /// # Parameters
  ///
  /// * `bit` - The index of the bit.
  pub fn set_bit(&mut self, bit: usize) {
    if bit >= self.bits {
      return;
    }

    let (word, shift) = self.get_word_and_shift(bit);
    self.bitmap[word] |= 1 << shift;
  }

  /// Set all bits in the mask.
  pub fn set_all_bits(&mut self) {
    for word in self.bitmap.iter_mut() {
      *word = usize::MAX;
    }
  }

  /// Clear a bit in the mask.
  ///
  /// # Parameters
  ///
  /// * `bit` - The index of the bit.
  pub fn clear_bit(&mut self, bit: usize) {
    if bit >= self.bits {
      return;
    }

    let (word, shift) = self.get_word_and_shift(bit);
    self.bitmap[word] &= !(1 << shift);
  }

  /// Clear all bits in the mask.
  pub fn clear_all_bits(&mut self) {
    for word in self.bitmap.iter_mut() {
      *word = 0;
    }
  }

  /// Toggle a bit in the mask.
  ///
  /// # Parameters
  ///
  /// * `bit` - The index of the bit.
  pub fn toggle_bit(&mut self, bit: usize) {
    if bit >= self.bits {
      return;
    }

    let (word, shift) = self.get_word_and_shift(bit);
    self.bitmap[word] ^= 1 << shift;
  }

  /// Toggle all bits in the mask.
  pub fn toggle_all_bits(&mut self) {
    for word in self.bitmap.iter_mut() {
      *word ^= usize::MAX;
    }
  }

  /// Test a bit in the mask.
  ///
  /// # Parameters
  ///
  /// * `bit` - The index of the bit.
  ///
  /// # Returns
  ///
  /// True or false if the bit is 1 or 0 respectively, or None if `bit` is
  /// outside the map.
  pub fn test_bit(&self, bit: usize) -> Option<bool> {
    if bit >= self.bits {
      return None;
    }

    let (word, shift) = self.get_word_and_shift(bit);
    Some(self.bitmap[word] & (1 << shift) != 0)
  }

  /// Get the index of the first zero bit.
  ///
  /// # Returns
  ///
  /// The index of the first zero bit, or None if all bits are 1.
  pub fn first_zero(&self) -> Option<usize> {
    let mut index = 0;

    for w in 0..self.bitmap.len() {
      // Invert the word and get the number of trailing zeros. This will be the
      // index of the first zero. For example: b10110111. Inverting gives:
      // b01001000. There are three trailing zeros in the *inverted* word, so
      // the first zero is at index 3.
      let z = (!self.bitmap[w]).trailing_zeros() as usize;

      // If z is less than WORD_BITS, then the first zero is in this word.
      if z < WORD_BITS {
        index += z;
        break;
      }

      // Otherwise, increment the index by WORD_BITS and try the next word.
      index += WORD_BITS;
    }

    // Bits are initialized to zero, so the first zero may not be in the map.
    if index >= self.bits {
      return None;
    }

    Some(index)
  }

  /// Helper to get the word and shift of a bit.
  ///
  /// # Assumptions
  ///
  /// Assumes the bit has been checked.
  fn get_word_and_shift(&self, bit: usize) -> (usize, usize) {
    let word = bit >> WORD_BIT_SHIFT;
    let shift = bit & WORD_BIT_MASK;
    (word, shift)
  }
}

impl<'a, const MAP_SIZE: usize> IntoIterator for &'a Bitmap<MAP_SIZE> {
  type Item = usize;
  type IntoIter = BitmapIter<'a, MAP_SIZE>;

  /// See `IntoIter::into_iter`.
  fn into_iter(self) -> BitmapIter<'a, MAP_SIZE> {
    BitmapIter {
      index: 0,
      word: 0,
      bit: 0,
      bitmap: self,
    }
  }
}

/// A bitmap iterator that iterates over bits that are *true* in the map.
pub struct BitmapIter<'a, const MAP_WORDS: usize> {
  index: usize,
  word: usize,
  bit: usize,
  bitmap: &'a Bitmap<MAP_WORDS>,
}

impl<'a, const MAP_WORDS: usize> Iterator for BitmapIter<'a, MAP_WORDS> {
  type Item = usize;

  /// Get the index of the next bit set in the map. See `Iterator::next`.
  fn next(&mut self) -> Option<Self::Item> {
    while self.index < self.bitmap.bits {
      // Save off the current index and value.
      let index = self.index;
      let val = self.bitmap.bitmap[self.word] & (1 << index) != 0;

      // Add one to the index and bit. If the bit rolls over, move to the next
      // word in the map.
      self.index += 1;

      if self.bit.wrapping_add(1) == 0 {
        self.word += 1;
      }

      // If the bit is set, return the index.
      if val {
        return Some(index);
      }
    }

    // No more bits set.
    None
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  let mut context = test::TestContext::new();
  tests::run_bitmap_tests(&mut context);
  debug_print!(" bits: {} pass, {} fail\n", context.pass_count, context.fail_count);
}
