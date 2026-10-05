//! AArch64 I/O Memory Management

/// Initialize the I/O module.
///
/// # Description
///
///   NOTE: Must only be called from the main io module during initialization.
pub fn init() {}

/// Map a block of pages as I/O memory.
///
/// # Parameters
///
/// * `phys_addr` - The base physical address of the block.
/// * `pages` - The number of pages to map.
///
/// # Description
///
///   TODO: Should AArch64 have a block of reserved virtual addresses for I/O
///         mapping, or should it just linearly map? If it just linearly maps,
///         it probably still needs to make sure the mapping does not conflict
///         with any existing I/O mappings or any physical memory mappings.
///
/// # Returns
///
/// The virtual address of the mapped block, or None if no virtual address
/// blocks of the required size are available.
pub fn map_io(phys_addr: usize, pages: usize) -> Option<usize> {
  todo!()
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
  todo!()
}
