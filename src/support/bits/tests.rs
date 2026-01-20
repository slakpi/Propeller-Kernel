use super::{Bitmap, WORD_BIT_SHIFT, WORD_BITS};
use crate::support::bits;
use crate::{check_eq, check_neq, check_none, check_optional, execute_test, mark_fail, test};

/// Maximum number of bits to store.
const TEST_MAX_BITS: usize = 128;

/// The test map size in words.
const TEST_MAP_SIZE: usize = TEST_MAX_BITS >> WORD_BIT_SHIFT;

/// The maximum number of bits to use.
const TEST_BITS: usize = 32;

/// Run the Bitmap tests.
///
/// # Parameters
///
/// * `context` - The test context.
pub fn run_bitmap_tests(context: &mut test::TestContext) {
  execute_test!(context, test_construction);
  execute_test!(context, test_bit_set);
  execute_test!(context, test_bit_clear);
  execute_test!(context, test_bit_toggle);
  execute_test!(context, test_bit_test);
  execute_test!(context, test_first_zero);
  execute_test!(context, test_bit_iterator);
}

/// Test construction of a Bitmap.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_construction(context: &mut test::TestContext) {
  let normal = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);
  check_eq!(context, normal.len(), TEST_BITS);

  let too_many = Bitmap::<TEST_MAP_SIZE>::new(TEST_MAX_BITS * 2);
  check_eq!(context, too_many.len(), TEST_MAX_BITS);
}

/// Test the bit set operation.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bit_set(context: &mut test::TestContext) {
  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Verify we can set a valid bit.
  map.set_bit(TEST_BITS >> 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS >> 1);
  check_eq!(context, map.bitmap[word], 1usize << shift);

  // verify we can set the last bit in the map.
  map.clear_all_bits();
  map.set_bit(TEST_BITS - 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS - 1);
  check_eq!(context, map.bitmap[word], 1usize << shift);

  // Verify we cannot set a bit past the end of the map.
  map.clear_all_bits();
  map.set_bit(TEST_BITS);
  let (word, shift) = map.get_word_and_shift(TEST_BITS);
  check_neq!(context, map.bitmap[word], 1usize << shift);

  // Test setting all bits.
  map.clear_all_bits();
  for word in map.bitmap {
    assert_eq!(word, 0);
  }
  map.set_all_bits();
  for word in map.bitmap {
    check_eq!(context, word, usize::MAX);
  }
}

/// Test the bit clear operation.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bit_clear(context: &mut test::TestContext) {
  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Verify we can clear a bit.
  map.set_all_bits();
  map.clear_bit(TEST_BITS >> 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS >> 1);
  check_eq!(context, map.bitmap[word], usize::MAX & !(1usize << shift));

  // Verify that we can clear the last bit in the map.
  map.set_all_bits();
  map.clear_bit(TEST_BITS - 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS - 1);
  check_eq!(context, map.bitmap[word], usize::MAX & !(1usize << shift));

  // Verify we cannot clear a bit past the end of the map.
  map.set_all_bits();
  map.clear_bit(TEST_BITS);
  let (word, shift) = map.get_word_and_shift(TEST_BITS);
  check_eq!(context, map.bitmap[word], usize::MAX);

  // Test clearing all bits.
  map.set_all_bits();
  for word in map.bitmap {
    assert_eq!(word, usize::MAX);
  }
  map.clear_all_bits();
  for word in map.bitmap {
    check_eq!(context, word, 0);
  }
}

/// Test the bit toggle operation.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bit_toggle(context: &mut test::TestContext) {
  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Verify we can toggle a bit on.
  map.toggle_bit(TEST_BITS >> 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS >> 1);
  check_eq!(context, map.bitmap[word], 1 << shift);

  // Verify we can toggle a bit off.
  map.set_all_bits();
  map.toggle_bit(TEST_BITS >> 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS >> 1);
  check_eq!(context, map.bitmap[word], usize::MAX & !(1 << shift));

  // Verify that we can toggle the last bit in the map.
  map.set_all_bits();
  map.toggle_bit(TEST_BITS - 1);
  let (word, shift) = map.get_word_and_shift(TEST_BITS - 1);
  check_eq!(context, map.bitmap[word], usize::MAX & !(1usize << shift));

  // Verify we cannot toggle a bit past the end of the map.
  map.set_all_bits();
  map.toggle_bit(TEST_BITS);
  let (word, shift) = map.get_word_and_shift(TEST_BITS);
  check_eq!(context, map.bitmap[word], usize::MAX);

  // Test toggling all bits.
  let pattern = bits::interleave_bits(usize::MAX, 0);
  for word in map.bitmap.iter_mut() {
    *word = pattern;
  }
  map.toggle_all_bits();
  for word in map.bitmap {
    check_eq!(context, word, !pattern);
  }
}

/// Test the bit test operation.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bit_test(context: &mut test::TestContext) {
  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Verify testing a true bit returns true.
  map.set_bit(TEST_BITS >> 1);
  let t = map.test_bit(TEST_BITS >> 1);
  check_optional!(context, t, true);

  // Verify testing a false bit returns false.
  map.set_all_bits();
  map.clear_bit(TEST_BITS >> 1);
  let t = map.test_bit(TEST_BITS >> 1);
  check_optional!(context, t, false);

  // Verify we can test the last bit in the map.
  map.set_all_bits();
  let t = map.test_bit(TEST_BITS - 1);
  check_optional!(context, t, true);

  // Verify we cannot test a bit past the end of the map.
  map.set_all_bits();
  let t = map.test_bit(TEST_BITS);
  check_none!(context, t);
}

/// Test finding the first zero.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_first_zero(context: &mut test::TestContext) {
  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Set all available bits and verify the unused zeros are not counted.
  for i in 0..TEST_BITS {
    map.set_bit(i);
  }
  let t = map.first_zero();
  check_none!(context, t);

  // Verify the first bit.
  map.set_all_bits();
  map.clear_bit(0);
  let t = map.first_zero();
  check_optional!(context, t, 0);

  // Verify a middle bit.
  map.set_all_bits();
  map.clear_bit(TEST_BITS >> 1);
  let t = map.first_zero();
  check_optional!(context, t, TEST_BITS >> 1);

  // Verify the last bit.
  map.set_all_bits();
  map.clear_bit(TEST_BITS - 1);
  let t = map.first_zero();
  check_optional!(context, t, TEST_BITS - 1);

  // Verify multiple zeros.
  map.set_all_bits();
  map.clear_bit(TEST_BITS - 1);
  map.clear_bit(TEST_BITS - 2);
  let t = map.first_zero();
  check_optional!(context, t, TEST_BITS - 2);
}

/// Test iterating over set bits.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bit_iterator(context: &mut test::TestContext) {
  const INDICES: [usize; 3] = [0, TEST_BITS >> 1, TEST_BITS - 1];

  let mut map = Bitmap::<TEST_MAP_SIZE>::new(TEST_BITS);

  // Verify exactly three iterations and their indices.
  map.set_bit(0);
  map.set_bit(TEST_BITS >> 1);
  map.set_bit(TEST_BITS - 1);
  for (index, t) in map.into_iter().enumerate() {
    if index >= INDICES.len() {
      mark_fail!(context, "Too many iterations.");
      break;
    }

    check_eq!(context, t, INDICES[index]);
  }

  // Verify zero iterations.
  map.clear_all_bits();
  for t in &map {
    mark_fail!(context, "Too many iterations.");
    break;
  }

  // Verify iterating over all bits.
  let mut count = 0;
  map.set_all_bits();
  for _ in &map {
    count += 1;
  }
  check_eq!(context, count, TEST_BITS);
}
