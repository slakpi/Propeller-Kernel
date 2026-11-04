//! AArch64 I/O Memory Management

use crate::arch::{self, cpu};
use crate::support::memory::{MappingStrategy, MemoryZone, PageAllocator};

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
/// * `allocator` - The allocator that will provide new table pages.
pub fn map_io(
  phys_addr: usize,
  virt_addr: usize,
  pages: usize,
  allocator: &mut impl PageAllocator,
) {
  super::mm::map_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    phys_addr,
    pages << super::init::get_page_shift(),
    true,
    allocator,
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
/// * `allocator` - The allocator that will provide new table pages.
///
/// # Assumptions
///
/// Assumes the virtual address is an address returned by `map_io()`.
pub fn unmap_io(virt_addr: usize, pages: usize, allocator: &mut impl PageAllocator) {
  super::mm::unmap_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    pages << super::init::get_page_shift(),
    allocator,
  );

  cpu::flush_tlb_by_va_range(virt_addr, pages, 0);
  cpu::sync_barriers();
}
