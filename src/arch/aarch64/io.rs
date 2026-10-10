//! AArch64 I/O Memory Management

use crate::arch::cpu;
use crate::mm;
use crate::support::memory::{MappingStrategy, MemoryZone};

/// Initialize the I/O memory management module.
///
/// # Description
///
///   TODO: Determine the I/O mapping area. This could probably just be a 1 GiB
///         area below the page database.
///
/// # Returns
///
/// The base virtual address and size, in bytes, of the driver area.
pub fn init() -> (usize, usize) {
  let base = super::init::get_driver_area_virtual_base();
  let size = super::init::get_driver_area_size();
  (base, size)
}

/// Map a block of pages as I/O memory.
///
/// # Parameters
///
/// * `phys_addr` - The base physical address of the block.
/// * `virt_addr` - The base virtual address of the block.
/// * `pages` - The number of pages to map.
///
/// # Assumptions
///
/// Assumes the function has exclusive access to the page tables.
pub fn map_io(phys_addr: usize, virt_addr: usize, pages: usize) {
  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();

  super::mm::map_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    phys_addr,
    pages << super::init::get_page_shift(),
    true,
    alloc.lock().as_mut(),
    MappingStrategy::Granular,
  );

  cpu::flush_tlb_by_va_range(virt_addr, pages, 0);
  cpu::sync_barriers();
}

/// Unmap a block of I/O memory.
///
/// # Parameters
///
/// * `virt_addr` - The base virtual address of the block.
/// * `pages` - The number of pages to map.
///
/// # Description
///
/// The virtual address must be an address returned by `map_io()`.
///
/// # Assumptions
///
/// Assumes the function has exclusive access to the page tables.
pub fn unmap_io(virt_addr: usize, pages: usize) {
  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();

  super::mm::unmap_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    pages << super::init::get_page_shift(),
    alloc.lock().as_mut(),
  );

  cpu::flush_tlb_by_va_range(virt_addr, pages, 0);
  cpu::sync_barriers();
}
