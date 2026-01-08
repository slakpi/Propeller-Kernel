//! Bit manipulation utilities.
//!
//! http://aggregate.org/MAGIC/
//! http://graphics.stanford.edu/~seander/bithacks.html

pub use crate::arch::bits::*;

/// The number of bytes in a machine word.
pub const WORD_BYTES: usize = (usize::BITS / 8) as usize;

/// The machine word byte size shift.
pub const WORD_SHIFT: usize = floor_log2(WORD_BYTES);

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

/// Set a bit in a multi-word mask.
///
/// # Parameters
///
/// * `mask` - The mask.
/// * `bit` - The bit to set.
pub fn set_bit(mask: &mut [usize], bit: usize) {
  let word = bit / usize::BITS as usize;
  let shift = bit % usize::BITS as usize;
  mask[word] |= 1 << shift;
}

/// Clear a bit in a multi-word mask.
///
/// # Parameters
///
/// * `mask` - The mask.
/// * `bit` - The bit to clear.
pub fn clear_bit(mask: &mut [usize], bit: usize) {
  let word = bit / usize::BITS as usize;
  let shift = bit % usize::BITS as usize;
  mask[word] &= !(1 << shift);
}
