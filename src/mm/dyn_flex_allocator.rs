//! Dynamic Flex Allocator

use super::page_allocator::BuddyPageAllocator;
use super::{BlockAllocator, FlexAllocator, PageAllocator};
use crate::sync::SpinLock;
use core::ptr;

/// Dynamic flex allocator. Performs buffered single-page allocations and
/// unbuffered block allocations. The dynamic flex allocator does not perform
/// page allocations directly. It uses a callback to obtain a reference to a
/// a page allocator that performs the allocation work.
///
/// Buffered single-page allocations generally do not incur any synchronization
/// overhead unless the buffer needs to be refilled. Unbuffered block
/// allocations must lock the global allocator.
///
///   NOTE: The allocator is NOT thread-safe.
pub struct DynamicFlexAllocator<'alloc, const BUFFER_PAGE_COUNT: usize> {
  get_allocator_cb: fn() -> &'alloc mut SpinLock<BuddyPageAllocator<'alloc>>,
  page_buffer: [usize; BUFFER_PAGE_COUNT],
  buffer_count: usize,
}

impl<'alloc, const BUFFER_PAGE_COUNT: usize> DynamicFlexAllocator<'alloc, BUFFER_PAGE_COUNT> {
  /// Convenience buffer initializer.
  const PAGE_BUFFER_INITIALIZER: [usize; BUFFER_PAGE_COUNT] = [0; BUFFER_PAGE_COUNT];

  /// Construct a new linear flex allocator.
  ///
  /// # Parameters
  ///
  /// * `get_allocator_cb` - Global allocator callback.
  pub const fn new(
    get_allocator_cb: fn() -> &'alloc mut SpinLock<BuddyPageAllocator<'alloc>>,
  ) -> Self {
    Self {
      get_allocator_cb,
      page_buffer: Self::PAGE_BUFFER_INITIALIZER,
      buffer_count: 0,
    }
  }

  /// Buffered free helper.
  fn buffered_free(&mut self, addr: usize) -> bool {
    if self.buffer_count >= BUFFER_PAGE_COUNT {
      return false;
    }

    self.page_buffer[self.buffer_count] = addr;
    self.buffer_count += 1;
    true
  }

  /// Unbuffered allocation helper.
  fn unbuffered_alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    let mut alloc = (self.get_allocator_cb)().lock();
    alloc.allocate(pages)
  }

  /// Unbuffered free helper.
  fn unbuffered_free(&mut self, addr: usize, pages: usize) {
    let mut alloc = (self.get_allocator_cb)().lock();
    alloc.free(addr, pages);
  }
}

impl<'alloc, const BUFFER_PAGE_COUNT: usize> PageAllocator
  for DynamicFlexAllocator<'alloc, BUFFER_PAGE_COUNT>
{
  /// See `PageAllocator::alloc`.
  fn alloc(&mut self) -> Option<usize> {
    // Attempt to refill the page buffer.
    if self.buffer_count == 0 {
      let mut alloc = (self.get_allocator_cb)().lock();

      while self.buffer_count < BUFFER_PAGE_COUNT {
        let addr = alloc.allocate(1);

        if addr.is_none() {
          break;
        }

        self.page_buffer[self.buffer_count] = addr.unwrap().0;
        self.buffer_count += 1;
      }
    }

    // If the buffer is still empty, there are no free pages.
    if self.buffer_count == 0 {
      return None;
    }

    // Get a page from the buffer.
    self.buffer_count -= 1;
    Some(self.page_buffer[self.buffer_count])
  }

  /// See `PageAllocator::free`.
  fn free(&mut self, addr: usize) {
    // If the addr is zero, there is nothing to do.
    if addr == 0 {
      return;
    }

    // Add the page back to the buffer if able.
    if self.buffered_free(addr) {
      return;
    }

    // Otherwise, give the page back to the linear memory allocator.
    self.unbuffered_free(addr, 1);
  }
}

impl<'alloc, const BUFFER_PAGE_COUNT: usize> BlockAllocator
  for DynamicFlexAllocator<'alloc, BUFFER_PAGE_COUNT>
{
  /// See `BlockAllocator::contiguous_alloc`.
  fn contiguous_alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    // If pages is zero, there is nothing to do.
    if pages == 0 {
      return None;
    }

    // If requesting a single page, go the buffered route.
    if pages == 1 {
      if let Some(addr) = self.alloc() {
        return Some((addr, 1));
      }

      return None;
    }

    // Otherwise, request a block from the linear memory allocator.
    self.unbuffered_alloc(pages)
  }

  /// See `BlockAllocator::contiguous_free`.
  fn contiguous_free(&mut self, addr: usize, pages: usize) {
    // If the addr or page count is zero, there is nothing to do.
    if addr == 0 || pages == 0 {
      return;
    }

    // If freeing a single page, just add it to the buffer if able to avoid
    // locking the linear memory allocator.
    if pages == 1 && self.buffered_free(addr) {
      return;
    }

    // Otherwise, give the page(s) back to the linear memory allocator.
    self.unbuffered_free(addr, pages);
  }
}

impl<'alloc, const BUFFER_PAGE_COUNT: usize> FlexAllocator
  for DynamicFlexAllocator<'alloc, BUFFER_PAGE_COUNT>
{
}

impl<'alloc, const BUFFER_PAGE_COUNT: usize> Drop
  for DynamicFlexAllocator<'alloc, BUFFER_PAGE_COUNT>
{
  /// Release all buffered pages.
  fn drop(&mut self) {
    let mut alloc = (self.get_allocator_cb)().lock();

    for i in 0..self.buffer_count {
      alloc.free(self.page_buffer[i], 1);
    }
  }
}
