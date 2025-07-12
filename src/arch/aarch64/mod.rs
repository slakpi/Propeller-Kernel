//! AArch64 Architecture

pub mod exceptions;

use crate::support::bits;

/// Basic kernel configuration provided by the start code. All address are
/// physical.
#[repr(C)]
#[derive(Copy, Clone)]
struct KernelConfig {
  virtual_base: usize,
  page_size: usize,
  blob: usize,
  kernel_base: usize,
  kernel_size: usize,
  kernel_pages_start: usize,
  kernel_pages_size: usize,
  kernel_stack_list: usize,
  kernel_stack_pages: usize,
  primary_stack_start: usize,
}

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// Kernel configuration provided by the start code.
static mut KERNEL_CONFIG: KernelConfig = KernelConfig {
  virtual_base: 0,
  page_size: 0,
  blob: 0,
  kernel_base: 0,
  kernel_size: 0,
  kernel_pages_start: 0,
  kernel_pages_size: 0,
  kernel_stack_list: 0,
  kernel_stack_pages: 0,
  primary_stack_start: 0,
};

/// AArch64 platform configuration.
///
/// # Parameters
///
/// * `config` - The kernel configuration address provided by the start code.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
///
///   NOTE: Assumes 4 KiB pages.
///
///   NOTE: Assumes the kernel stack page count is a power of two.
///
///   NOTE: Assumes the blob is a DTB.
pub fn init(config: usize) {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  assert_ne!(config, 0);

  let kconfig = unsafe { &*(config as *const KernelConfig) };

  assert_eq!(kconfig.page_size, 4096);
  assert!(bits::is_power_of_2(kconfig.kernel_stack_pages));

  unsafe {
    KERNEL_CONFIG = *kconfig;
  }
}
