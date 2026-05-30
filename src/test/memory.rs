//! Test Memory Utilities

use crate::arch;
use crate::arch::memory::{MemoryRange, MemoryZone, PageAllocator};
use crate::mm::page_allocator::BuddyPageAllocator;
use crate::support::bits;

/// Test page size.
///
///   TODO: This is not a super great idea. The tests should probably not assume
///         a specific page size.
pub const PAGE_SIZE: usize = 4096;

/// Test page shift.
pub const PAGE_SHIFT: usize = bits::floor_log2(PAGE_SIZE);

/// Provide 9 MiB of test memory.
pub const PAGE_COUNT: usize = 2304;

/// Total memory available in bytes.
pub const MEMORY_SIZE: usize = PAGE_SIZE * PAGE_COUNT;

/// Size of the buddy page allocator metadata.
const META_SIZE: usize = BuddyPageAllocator::calc_metadata_size(MEMORY_SIZE);

/// Use the whole test buffer minus the metadata for the page allocator.
const ALLOC_SIZE: usize = bits::align_down(MEMORY_SIZE - META_SIZE, arch::get_page_size());

/// Alignment type.
#[repr(align(0x400000))]
struct _Align4MiB;

/// Wrapper type to align the memory block.
struct _MemWrapper {
  _alignment: [_Align4MiB; 0],
  mem: [u8; MEMORY_SIZE],
}
/// The Test Allocator either blocks allocations to simulate low memory, or it
/// passes allocation requests through to a real allocator.
pub struct TestPageAllocator {
  allocator: BuddyPageAllocator<'static>,
  can_alloc: bool,
  alloc_count: usize,
  alloc_total: usize,
  free_count: usize,
  free_total: usize,
}

impl TestPageAllocator {
  /// Construct a new Test Allocator.
  pub fn new(allocator: BuddyPageAllocator<'static>) -> Self {
    Self {
      allocator,
      can_alloc: true,
      alloc_count: 0,
      alloc_total: 0,
      free_count: 0,
      free_total: 0,
    }
  }

  /// Set the allocation pass-through state.
  ///
  /// # Parameters
  ///
  /// * `can_alloc` - Whether the allocator should allow allocations.
  pub fn set_can_alloc(&mut self, can_alloc: bool) {
    self.can_alloc = can_alloc;
  }

  /// Get the allocation count.
  pub fn get_alloc_count(&self) -> usize {
    self.alloc_count
  }

  /// Get the total number of pages requested.
  pub fn get_alloc_total(&self) -> usize {
    self.alloc_total
  }

  /// Get the free count.
  pub fn get_free_count(&self) -> usize {
    self.free_count
  }

  /// Get the total number of pages freed.
  pub fn get_free_total(&self) -> usize {
    self.free_total
  }

  /// Reset counters.
  pub fn reset_counters(&mut self) {
    self.alloc_count = 0;
    self.alloc_total = 0;
    self.free_count = 0;
    self.free_total = 0;
  }
}

impl PageAllocator for TestPageAllocator {
  /// See `PageAllocator::MAX_BLOCK_PAGES`.
  const MAX_BLOCK_PAGES: usize = BuddyPageAllocator::MAX_BLOCK_PAGES;

  /// See `PageAllocator::alloc()`.
  fn alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    if !self.can_alloc {
      return None;
    }

    self.alloc_count += 1;
    self.alloc_total += 1 << bits::ceil_log2(pages);
    self.allocator.alloc(pages)
  }

  /// See `PageAllocator::free()`.
  fn free(&mut self, addr: usize, pages: usize) {
    self.free_count += 1;
    self.free_total += pages;
    self.allocator.free(addr, pages);
  }

  /// See `PageAllocator::get_alloc_mem()`.
  fn get_alloc_mem(&self) -> usize {
    self.allocator.get_alloc_mem()
  }

  /// See `PageAllocator::get_free_mem()`.
  fn get_free_mem(&self) -> usize {
    self.allocator.get_free_mem()
  }
}

/// Statically allocate the test memory as part of the kernel image.
static mut TEST_MEM: _MemWrapper = _MemWrapper {
  _alignment: [],
  mem: [0; MEMORY_SIZE],
};

/// Get the static memory block.
pub fn get_test_memory_mut() -> &'static mut [u8] {
  unsafe { &mut (*(&raw mut TEST_MEM)).mem }
}

/// Reset the contents of the static memory block.
pub fn reset_test_memory() {
  let mem = get_test_memory_mut();
  mem.fill(0);
}

/// Construct a test page allocator.
///
/// # Description
///
/// Constructs a test page allocator with a single available region.
///
///     |----------- MEMORY_SIZE ------------|
///
///            ALLOC_SIZE
///     +-------------------------+----------+
///     | Available Region        | Metadata |
///     +-------------------------+----------+
///
/// # Returns
///
/// The new page allocator.
pub fn make_test_page_allocator() -> TestPageAllocator {
  let virt_base = arch::get_kernel_virtual_base();
  let phys_addr = get_test_memory_mut().as_ptr() as usize - virt_base;
  let meta_addr = virt_base + phys_addr + ALLOC_SIZE;

  reset_test_memory();

  let avail = &[MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: phys_addr,
    size: ALLOC_SIZE,
  }];

  // Assume this will never fail. If it does, something is wrong with the test
  // setup.
  TestPageAllocator::new(
    BuddyPageAllocator::new(phys_addr, ALLOC_SIZE, meta_addr as *mut u8, avail).unwrap(),
  )
}
