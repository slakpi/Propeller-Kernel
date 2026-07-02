//! 64-bit Bit Manipulation Utilities
//!
//! http://aggregate.org/MAGIC/
//! http://graphics.stanford.edu/~seander/bithacks.html
//! https://stackoverflow.com/questions/45694690/how-i-can-remove-all-odds-bits-in-c

/// Random seed bytes for a checksum.
pub const CHECKSUM_SEED: usize = 0x09a5_2af1_c62b_d04b;

/// Poison value.
pub const POISON_WORD: usize = 0xcccc_cccc_cccc_cccc;

/// Poison byte value.
pub const POISON_BYTE: u8 = 0xcc;

/// Fast 64-bit population count.
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
  n -= (n >> 1) & 0x5555_5555_5555_5555;
  n = ((n >> 2) & 0x3333_3333_3333_3333) + (n & 0x3333_3333_3333_3333);
  n = ((n >> 4) + n) & 0x0f0f_0f0f_0f0f_0f0f;
  n += n >> 8;
  n += n >> 16;
  n += n >> 32;

  n & 0x7f
}

/// Fast 64-bit floor base-2 log of a number.
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
  n |= n >> 32;

  ones(n >> 1)
}

/// Fast 64-bit ceiling base-2 log of a number.
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
  m >>= 63;

  let mut n = n;
  n |= n >> 1;
  n |= n >> 2;
  n |= n >> 4;
  n |= n >> 8;
  n |= n >> 16;
  n |= n >> 32;

  ones(n >> 1) + m
}

/// Interleaves the lower 32-bit bits of two numbers.
///
/// # Parameters
///
/// * `a` - The first number.
/// * `b` - The second number.
///
/// # Description
///
/// Given 0xxxxx_xxxx_aaaa_aaaa and 0xxxxx_xxxx_bbbb_bbbb where `x` is ignored,
/// the result is 0xbaba_baba_baba_baba.
///
/// # Returns
///
/// The interleaved bits.
pub const fn interleave_bits(a: usize, b: usize) -> usize {
  const B: [usize; 5] = [
    0x5555_5555_5555_5555,
    0x3333_3333_3333_3333,
    0x0f0f_0f0f_0f0f_0f0f,
    0x00ff_00ff_00ff_00ff,
    0x0000_ffff_0000_ffff,
  ];
  const S: [usize; 5] = [1, 2, 4, 8, 16];

  let mut x = a;
  let mut y = b;

  x = (x | (x << S[4])) & B[4];
  x = (x | (x << S[3])) & B[3];
  x = (x | (x << S[2])) & B[2];
  x = (x | (x << S[1])) & B[1];
  x = (x | (x << S[0])) & B[0];

  y = (y | (y << S[4])) & B[4];
  y = (y | (y << S[3])) & B[3];
  y = (y | (y << S[2])) & B[2];
  y = (y | (y << S[1])) & B[1];
  y = (y | (y << S[0])) & B[0];

  x | (y << 1)
}
