//! Architecture Module

// Common module for all architectures.
pub mod bits;
pub mod cpu;
pub mod debug;
pub mod interrupts;
pub mod io;
pub mod sync;
pub mod task;

// Architecture-specific modules. These are imported as private modules. The
// required public functions common to all architectures are implemented below
// by calling into the appropriate architecture module instead of publicly using
// the contents of the architecture's init module. This allows the architecture
// to have functions it uses internally without exporting them to the rest of
// the kernel.
#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(target_arch = "arm")]
mod arm;
#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
mod arm_common;

use crate::support::device_tree;
use crate::support::memory::PageAllocator;
use crate::sync::SpinLock;
use core::ptr;

// Reimport the architecture module with a consistent name.
#[cfg(target_arch = "aarch64")]
use aarch64 as intf;
#[cfg(target_arch = "arm")]
use arm as intf;

/// Single-threaded architecture initialization.
///
/// # Parameters
///
/// * `config_addr` - The physical kernel configuration address provided by the
/// start code.
pub fn init(config_addr: usize) {
  intf::init::init(config_addr);
}

/// Initialize symmetric multiprocessing.
///
/// # Parameters
///
/// * `allocator` - An allocator suitable for allocating stacks and page tables.
pub fn init_smp(allocator: &SpinLock<impl PageAllocator>) {
  intf::init::init_smp(allocator);
}

/// Get the size of a page.
pub const fn get_page_size() -> usize {
  intf::init::get_page_size()
}

/// Get the page shift.
pub const fn get_page_shift() -> usize {
  intf::init::get_page_shift()
}

/// Get the page alignment mask.
pub const fn get_page_mask() -> usize {
  intf::init::get_page_mask()
}

/// Get the size of a section.
pub const fn get_section_size() -> usize {
  intf::init::get_section_size()
}

/// Get the section shift.
pub const fn get_section_shift() -> usize {
  intf::init::get_section_shift()
}

/// Get the section alignment mask.
pub const fn get_section_mask() -> usize {
  intf::init::get_section_mask()
}

/// Get the size of page table entry.
pub const fn get_page_table_entry_size() -> usize {
  intf::init::get_page_table_entry_size()
}

/// Get the page table entry shift.
pub const fn get_page_table_entry_shift() -> usize {
  intf::init::get_page_table_entry_shift()
}

/// Get the kernel base address.
pub fn get_kernel_base() -> usize {
  intf::init::get_kernel_base()
}

/// Get the maximum physical address.
pub fn get_maximum_physical_address() -> usize {
  intf::init::get_maximum_physical_address()
}

/// Get the kernel virtual base address.
pub fn get_kernel_virtual_base() -> usize {
  intf::init::get_kernel_virtual_base()
}

/// Get the system device tree.
pub fn get_device_tree() -> &'static device_tree::DeviceTree {
  intf::init::get_device_tree()
}

/// Get the core index of the current core.
///
/// # Description
///
/// For any non-trivial use of the core index, interrupts must be disabled prior
/// to calling to prevent the task from moving to another core.
pub fn get_current_core_index() -> usize {
  intf::init::get_current_core_index()
}

/// Get the page database virtual base address.
pub fn get_page_database_virtual_base() -> usize {
  intf::init::get_page_database_virtual_base()
}

/// Get the size of the page database.
pub fn get_page_database_size() -> usize {
  intf::init::get_page_database_size()
}

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  intf::run_tests();
}
