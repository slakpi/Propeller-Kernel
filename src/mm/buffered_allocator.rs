use crate::arch;
use crate::support::bits;
use crate::support::memory::PageAllocator;
use core::ptr;

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
  const MAX_BLOCK_PAGES: usize = 1;

  /// See `PageAllocator::alloc`.
  fn alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    if pages != 1 {
      return None;
    }

    let z = self.bitmap.first_zero()?;
    self.bitmap.set_bit(z);
    Some((self.start_addr + (z * self.page_size), 1))
  }

  /// See `PageAllocator::alloc_and_zero`.
  fn alloc_and_zero(&mut self, pages: usize) -> Option<(usize, usize)> {
    let (addr, pages) = self.alloc(pages)?;
    unsafe { ptr::write_bytes(addr as *mut u8, 0, pages * arch::get_page_size()) };
    Some((addr, pages))
  }

  /// See `PageAllocator::free`.
  fn free(&mut self, addr: usize, pages: usize) {
    assert_eq!(pages, 1);
    assert!(addr >= self.start_addr && addr < self.end_addr);
    assert!(bits::is_aligned(addr, self.page_size));
    let z = addr >> self.page_shift;
    self.bitmap.clear_bit(z);
  }

  /// Get the amount of memory currently allocated by this allocator in bytes.
  fn get_alloc_mem(&self) -> usize {
    let pages = self.bitmap.ones();
    pages << self.page_shift
  }

  /// Get the amount of memory currently available to this allocator in bytes.
  fn get_free_mem(&self) -> usize {
    let pages = self.bitmap.len() - self.bitmap.ones();
    pages << self.page_shift
  }
}
