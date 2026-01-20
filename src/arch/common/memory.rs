//! Common Memory Configuration Utilities

use crate::support::{bits, range, range_set};
use core::cmp;

/// Memory zone tags.
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum MemoryZone {
  /// Default for uninitialized allocators.
  InvalidZone,
  /// Linear memory zones are linearly mapped into the kernel's address space.
  /// Addresses in linear memory can be accessed by adding the kernel's base
  /// virtual address to a physical address.
  LinearMemoryZone,
  /// High memory is only meaningful on 32-bit architectures. High memory zones
  /// are not linearly mapped into the kernel's address space.
  HighMemoryZone,
}

/// Maximum number of memory ranges that can be stored in a configuration.
pub const MAX_MEM_RANGES: usize = 64;

/// Convenience range type.
pub type MemoryRange = range::Range<MemoryZone>;

/// Convenience range set type.
pub type MemoryConfig = range_set::RangeSet<MAX_MEM_RANGES, MemoryZone>;

/// Handles memory ranges as they are discovered.
pub trait MemoryRangeHandler {
  /// Performs any architecture-dependent processing on a range.
  ///
  /// # Parameters
  ///
  /// * `config` - The memory configuration to update.
  /// * `base` - The validated base of the range.
  /// * `size` - The validated size of the range.
  ///
  /// # Description
  ///
  /// The range will have already been validated to ensure the size is not 0,
  /// the base is not beyond usize::MAX and the range does not extend beyond
  /// usize::MAX.
  fn handle_range(&self, config: &mut MemoryConfig, base: usize, size: usize);
}

/// Mapping strategies to use when mapping blocks of memory.
pub enum MappingStrategy {
  /// A strategy that uses architecture-specific techniques, such as ARM
  /// sections, to map a block of memory using the fewest table entries.
  Compact,
  /// A strategy that maps a block of memory to individual pages.
  Granular,
}

/// Single-page allocator interface.
pub trait PageAllocator {
  /// Allocate a single page from linear memory.
  ///
  /// # Returns
  ///
  /// The physical address of a page in linear memory, or None if a page could
  /// not be allocated.
  fn alloc(&mut self) -> Option<usize>;

  /// Free a single page.
  ///
  /// # Parameters
  ///
  /// * `addr` - The physical address of the page.
  fn free(&mut self, addr: usize);
}

/// Contiguous page block allocator interface.
pub trait BlockAllocator {
  /// Allocate a physically-contiguous block of pages from linear memory.
  ///
  /// # Parameters
  ///
  /// * `pages` - The number of pages to allocate.
  ///
  /// # Returns
  ///
  /// A tuple with the physical base address of the block in linear memory and
  /// the actual number of pages allocated, or None if a block of the requested
  /// size could not be allocated.
  fn contiguous_alloc(&mut self, pages: usize) -> Option<(usize, usize)>;

  /// Free a contiguous block of page in linear memory.
  ///
  /// # Parameters
  ///
  /// * `addr` - The physical base address of the block.
  /// * `pages` - The number of pages to free.
  fn contiguous_free(&mut self, addr: usize, pages: usize);
}

/// The buffered page allocator provides pages from a pre-allocated block of
/// memory. The buffered page allocator only allocates single pages. The
/// allocator uses a bitmap of length BITMAP_WORDS to track allocated pages,
/// thus the allocator can track `BITMAP_WORDS << bits::WORD_BIT_SHIFT` pages.
pub struct BufferedPageAllocator<const BITMAP_WORDS: usize> {
  bitmap: bits::Bitmap<BITMAP_WORDS>,
  page_size: usize,
  page_shift: usize,
  start_addr: usize,
  end_addr: usize,
}

impl<const BITMAP_WORDS: usize> BufferedPageAllocator<BITMAP_WORDS> {
  const BITMAP_INITIALIZER: [usize; BITMAP_WORDS] = [0; BITMAP_WORDS];

  /// Construct a new allocator with a pre-allocated block of memory.
  ///
  /// # Parameters
  ///
  /// * `start_addr` - The starting physical address to use.
  /// * `end_addr` - The physical address of the first unavailable page.
  /// * `page_size` - The size of a page.
  ///
  /// # Description
  ///
  ///   NOTE: The start and end addresses must be page-aligned, and the end
  ///         address must be larger than the start address.
  ///
  ///   NOTE: The end address will be adjusted if it is beyond the number of
  ///         pages allowed by `BITMAP_WORDS`.
  ///
  ///   NOTE: The page size must be non-zero and a power of 2.
  ///
  /// # Assumptions
  ///
  /// The allocator assumes it has access to all pages in the range.
  pub fn new(start_addr: usize, end_addr: usize, page_size: usize) -> Self {
    assert!(bits::is_power_of_2(page_size));
    assert!(bits::is_aligned(start_addr, page_size));
    assert!(bits::is_aligned(end_addr, page_size));
    assert!(end_addr > start_addr);

    let page_shift = bits::floor_log2(page_size);
    let pages = (end_addr - start_addr) >> page_shift;

    Self {
      bitmap: bits::Bitmap::new(pages),
      page_size,
      page_shift,
      start_addr,
      end_addr,
    }
  }
}

impl<const BUFFER_SIZE: usize> PageAllocator for BufferedPageAllocator<BUFFER_SIZE> {
  /// See `PageAllocator::alloc`.
  fn alloc(&mut self) -> Option<usize> {
    if let Some(z) = self.bitmap.first_zero() {
      self.bitmap.set_bit(z);
      return Some(self.start_addr + (z * self.page_size));
    }

    None
  }

  /// See `PageAllocator::free`.
  fn free(&mut self, addr: usize) {
    assert!(addr >= self.start_addr && addr < self.end_addr);
    assert!(bits::is_aligned(addr, self.page_size));
    let z = addr >> self.page_shift;
    self.bitmap.clear_bit(z);
  }
}
