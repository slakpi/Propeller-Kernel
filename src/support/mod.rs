//! Support Module

use crate::debug_print;

pub mod bits;
pub mod debug;
pub mod dtb;
pub mod hash;
pub mod hash_map;
pub mod print;
pub mod range;
pub mod range_set;
pub mod vector;

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  let mut context = crate::test::TestContext::new();
  debug_print!(" support:\n");
  bits::run_tests(&mut context);
  vector::run_tests(&mut context);
  debug_print!("  {} pass, {} fail\n", context.pass_count, context.fail_count);
}
