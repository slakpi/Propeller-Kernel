//! I/O Memory Management

use crate::arch;
use crate::debug_print;

/// Re-initialization guard
static mut INITIALIZED: bool = false;

/// Initialize the I/O memory management module.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
pub fn init() {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  debug_print!("io init...\n");

  arch::io::init();
}
