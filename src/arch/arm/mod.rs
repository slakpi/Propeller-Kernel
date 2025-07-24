//! ARM Architecture

mod exceptions;

use crate::arch::cpu;
use crate::support::{bits, dtb};
use core::ptr;

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
  vm_split: usize,
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
  vm_split: 0,
  kernel_stack_list: 0,
  kernel_stack_pages: 0,
  primary_stack_start: 0,
};

/// CPU core configuration.
static mut CORE_CONFIG: cpu::CoreConfig = [cpu::Core::new(); cpu::MAX_CORES];

/// ARM platform configuration.
///
/// # Parameters
///
/// * `config_addr` - The physical kernel configuration address provided by the
/// start code.
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
pub fn init(config_addr: usize) {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  assert_ne!(config_addr, 0);

  let kconfig = unsafe { &*(config_addr as *const KernelConfig) };

  assert_eq!(kconfig.page_size, 4096);

  // Require a power-of-2 page count for the kernel stack size.
  assert!(bits::is_power_of_2(kconfig.kernel_stack_pages));

  // Calculate the blob address and its page-aligned size. There is no need to
  // do any real error checking on the size. The DTB reader will error check
  // during scans.
  let blob_vaddr = kconfig.virtual_base + kconfig.blob;
  let blob_size = dtb::DtbReader::check_dtb(blob_vaddr)
    .map_or_else(|_| 0, |size| bits::align_up(size, kconfig.page_size));

  // Validate the VM split and virtual base.
  assert!(
    (kconfig.vm_split == 3 && kconfig.virtual_base == 0xc000_0000)
      || (kconfig.vm_split == 2 && kconfig.virtual_base == 0x8000_0000)
  );

  unsafe {
    KERNEL_CONFIG = *kconfig;
  }

  init_core_config(blob_vaddr);
}

/// Get the number of cores.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_core_count() -> usize {
  unsafe { get_core_config().len() }
}

/// Get the full core configuration.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_core_config() -> &'static cpu::CoreConfig {
  unsafe { ptr::addr_of!(CORE_CONFIG).as_ref().unwrap() }
}

/// Initialize the core configuration.
///
/// # Parameters
///
/// * `blob_vaddr` - The DTB blob virtual address.
fn init_core_config(blob_vaddr: usize) {
  unsafe {
    assert!(cpu::get_core_config(ptr::addr_of_mut!(CORE_CONFIG).as_mut().unwrap(), blob_vaddr));
  }
}
