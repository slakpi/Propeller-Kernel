//! 32-bit Bit Manipulation Utilities
//!
//! http://aggregate.org/MAGIC/
//! http://graphics.stanford.edu/~seander/bithacks.html
//! https://stackoverflow.com/questions/45694690/how-i-can-remove-all-odds-bits-in-c

/// Random seed bytes for a checksum.
pub const CHECKSUM_SEED: usize = 0xe9da_acd5;

/// Fast 32-bit population count.
///
/// # Parameters
///
/// * `n` - The number.
///
/// # Returns
///
/// The number of bits set to 1 in the number.
pub const fn ones(n: usize) -> usize {
  let mut n = n;
  n -= (n >> 1) & 0x5555_5555;
  n = ((n >> 2) & 0x3333_3333) + (n & 0x3333_3333);
  n = ((n >> 4) + n) & 0x0f0f_0f0f;
  n += n >> 8;
  n += n >> 16;

  n & 0x3f
}

/// Fast 32-bit floor base-2 log of a number.
///
/// # Parameters
///
/// * `n` - The number.
///
/// # Returns
///
/// floor( log2( n ) ) when n > 0, 0 otherwise.
pub const fn floor_log2(n: usize) -> usize {
  let mut n = n;
  n |= n >> 1;
  n |= n >> 2;
  n |= n >> 4;
  n |= n >> 8;
  n |= n >> 16;

  ones(n >> 1)
}

/// Fast 32-bit ceiling base-2 log of an n.
///
/// # Parameters
///
/// * `n` - The number.
///
/// # Returns
///
/// ceiling( log2( n ) ) when n > 0, 0 otherwise.
pub const fn ceil_log2(n: usize) -> usize {
  let mut m = n & (n.wrapping_sub(1));
  m |= !m.wrapping_sub(1);
  m >>= 31;

  let mut n = n;
  n |= n >> 1;
  n |= n >> 2;
  n |= n >> 4;
  n |= n >> 8;
  n |= n >> 16;

  ones(n >> 1) + m
}

/// Removes all even (1-based) bits and leaves the odd bits in the lower
/// 16-bits.
///
/// # Parameters
///
/// * `n` - The number.
///
/// # Description
///
/// Given a 32-bit word 0xeoeoeoeo, where `e` is a 1-based even bit and `o` is
/// an odd bit, the function returns 0x0000oooo. Each odd bit maintains its
/// relative order with the other bits.
///
/// # Returns
///
/// The odd bits moved to the lower 16-bits.
pub const fn compact_odd_bits(n: usize) -> usize {
  let mut n = n;
  n = ((n & 0x4444_4444) >> 1) | (n & 0x1111_1111);
  n = ((n & 0x3030_3030) >> 2) | (n & 0x0303_0303);
  n = ((n & 0x0f00_0f00) >> 4) | (n & 0x000f_000f);
  n = ((n & 0x00ff_0000) >> 8) | (n & 0x0000_00ff);
  n
}

/// Removes all odd (1-based) bits and leaves the even bits in the lower
/// 16-bits.
///
/// # Parameters
///
/// * `n` - The number.
///
/// # Description
///
/// See `compact_odd_bits`.
///
/// # Returns
///
/// The even bits moved to the lower 16-bits.
pub const fn compact_even_bits(n: usize) -> usize {
  compact_odd_bits(n >> 1)
}
