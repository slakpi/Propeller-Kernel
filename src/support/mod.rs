//! Support Module

pub mod bits;
pub mod core_config;
pub mod debug;
pub mod device_tree;
pub mod dtb;
pub mod hash;
pub mod hash_map;
pub mod memory;
pub mod print;
pub mod range;
pub mod range_set;
pub mod rb_tree;
pub mod vector;

use crate::debug_print;

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  let mut context = crate::test::TestContext::new();
  debug_print!(" support:\n");
  bits::run_tests(&mut context);
  vector::run_tests(&mut context);
  rb_tree::run_tests(&mut context);
  debug_print!("  {} pass, {} fail\n", context.pass_count, context.fail_count);
}
