//! Page Table Allocator Utilities

use crate::support::bits;

/// Table allocator interface.
pub trait TableAllocator {
  /// Allocate a new table.
  ///
  /// # Returns
  ///
  /// The physical address of the new table, or None if unable to allocate a new
  /// table.
  fn alloc_table(&mut self) -> Option<usize>;
}

/// The linear table allocator accepts a pre-allocated block of memory and
/// incrementally allocates tables starting from the beginning of the block.
pub struct LinearTableAllocator {
  start_addr: usize,
  end_addr: usize,
  page_size: usize,
}

impl LinearTableAllocator {
  /// Construct a new allocator with a pre-allocated block of memory.
  ///
  /// # Parameters
  ///
  /// * `start_addr` - The first physical address to use for new tables.
  /// * `end_addr` - The physical address marking the end of the block.
  ///
  /// # Description
  ///
  ///   NOTE: The start address must be page-aligned.
  pub fn new(start_addr: usize, end_addr: usize, page_size: usize) -> Self {
    assert!(bits::is_power_of_2(page_size));
    assert!(bits::is_aligned(start_addr, page_size));

    Self {
      start_addr,
      end_addr,
      page_size,
    }
  }

  /// Get the current start address.
  pub fn get_start_address(&self) -> usize {
    self.start_addr
  }
}

impl TableAllocator for LinearTableAllocator {
  /// See `TableAllocator::alloc_table()`.
  fn alloc_table(&mut self) -> Option<usize> {
    if (self.start_addr >= self.end_addr) || (self.end_addr - self.start_addr < self.page_size) {
      return None;
    }

    let ret_addr = self.start_addr;
    self.start_addr += self.page_size;
    Some(ret_addr)
  }
}
