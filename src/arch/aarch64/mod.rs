//! AArch64 Architecture

pub mod exceptions;
pub mod init;
pub mod io;
pub mod mm;
pub mod task;

use crate::debug_print;

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  let mut context = crate::test::TestContext::new();
  debug_print!("arch:\n");
  task::run_tests(&mut context);
  debug_print!(" {} pass, {} fail\n", context.pass_count, context.fail_count);
}
