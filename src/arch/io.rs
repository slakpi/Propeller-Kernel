//! Architecture-Dependent I/O Memory Management

use super::intf;

/// Initialize the I/O memory management module.
pub fn init() {
  intf::io::init();
}

/// Map a block of pages as I/O memory.
///
/// # Parameters
///
/// * `phys_addr` - The base physical address of the block.
/// * `pages` - The number of pages to map.
///
/// # Returns
///
/// The virtual address of the mapped block, or None if no virtual address
/// blocks of the required size are available.
pub fn map_io(phys_addr: usize, pages: usize) -> Option<usize> {
  intf::io::map_io(phys_addr, pages)
}

/// Unmap a block of I/O memory.
///
/// # Parameters
///
/// * `virt_addr` - The base virtual address of the block.
///
/// # Description
///
/// The virtual address must be an address returned by `map_io()`.
pub fn unmap_io(virt_addr: usize) {
  intf::io::unmap_io(virt_addr);
}
