//! Memory Management

mod page_allocator;

use crate::arch;
use crate::arch::memory::{BlockAllocator, MemoryConfig, MemoryRange, MemoryZone, PageAllocator};
use crate::support::bits;
use crate::sync::{SpinLock, SpinLockGuard};
use core::ptr;
use page_allocator::BuddyPageAllocator;

/// Tracks the overall memory ranges covered by each zone and the required
/// metadata size.
struct ZoneInfo {
  range: MemoryRange,
  meta_size: usize,
  zone_index: usize,
  start_index: usize,
  end_index: usize,
}

/// Per-core page buffer size.
const PER_CORE_PAGE_BUFFER_SIZE: usize = 256;

/// Convenience type for the per-core linear allocators.
type PerCoreLinearAllocator = DynamicBufferedPageAllocator<PER_CORE_PAGE_BUFFER_SIZE>;

/// Total number of zone allocators and their indices.
const ZONE_ALLOCATOR_COUNT: usize = 2;

const LINEAR_MEMORY_ALLOCATOR: usize = 0;

const HIGH_MEMORY_ALLOCATOR: usize = 1;

/// Convenience initializer for the zone allocator array.
const ZONE_ALLOCATOR_INITIALIZER: Option<SpinLock<BuddyPageAllocator>> = None;

/// Convenience initializer for the per-core dynamic allocator array.
const DYNAMIC_MEMORY_ALLOCATOR_INITIALIZER: PerCoreLinearAllocator = PerCoreLinearAllocator::new();

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// The memory ranges served by the allocators minus metadata.
static mut ZONE_ALLOCATOR_MEMORY_CONFIG: MemoryConfig = MemoryConfig::new(MemoryZone::InvalidZone);

/// The zone allocators.
static mut ZONE_ALLOCATORS: [Option<SpinLock<BuddyPageAllocator>>; ZONE_ALLOCATOR_COUNT] =
  [ZONE_ALLOCATOR_INITIALIZER; ZONE_ALLOCATOR_COUNT];

/// The per-core dynamic allocators. These do not need to be protected by a spin
/// lock.
///
///   TODO: This probably is not a great way to do this, 256 allocators that
///         each buffer 256 pages comes to about 514 KiB. Might be able to just
///         allocate a page for each being used to limit the struct size to
///         a count, a page address, and maybe a slice reference for convenient
///         access.
static mut PER_CORE_ALLOCATORS: [PerCoreLinearAllocator; arch::cpu::MAX_CORES] =
  [DYNAMIC_MEMORY_ALLOCATOR_INITIALIZER; arch::cpu::MAX_CORES];

/// Dynamic, buffered page allocator. Performed buffered single-page
/// allocations and unbuffered block allocations from linear memory. The buffer
/// size is in words, so the maximum number of pages an allocator can buffer is
/// `BUFFER_SIZE << bits::WORD_BIT_SHIFT`.
pub struct DynamicBufferedPageAllocator<const BUFFER_SIZE: usize> {
  page_buffer: [usize; BUFFER_SIZE],
  buffer_count: usize,
}

impl<const BUFFER_SIZE: usize> DynamicBufferedPageAllocator<BUFFER_SIZE> {
  /// Convenience buffer initializer.
  const PAGE_BUFFER_INITIALIZER: [usize; BUFFER_SIZE] = [0; BUFFER_SIZE];

  /// Construct a new dynamic linear page allocator.
  pub const fn new() -> Self {
    Self {
      page_buffer: Self::PAGE_BUFFER_INITIALIZER,
      buffer_count: 0,
    }
  }

  /// Buffered free helper.
  fn buffered_free(&mut self, addr: usize) -> bool {
    if self.buffer_count >= BUFFER_SIZE {
      return false;
    }

    self.page_buffer[self.buffer_count] = addr;
    self.buffer_count += 1;
    true
  }

  /// Unbuffered allocation helper.
  fn unbuffered_alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    let allocators = unsafe { ptr::addr_of_mut!(ZONE_ALLOCATORS).as_mut().unwrap() };
    let alloc = allocators[LINEAR_MEMORY_ALLOCATOR].as_mut().unwrap();
    let mut guard = alloc.lock();
    guard.allocate(pages)
  }

  /// Unbuffered free helper.
  fn unbuffered_free(&mut self, addr: usize, pages: usize) {
    let allocators = unsafe { ptr::addr_of_mut!(ZONE_ALLOCATORS).as_mut().unwrap() };
    let alloc = allocators[LINEAR_MEMORY_ALLOCATOR].as_mut().unwrap();
    let mut guard = alloc.lock();
    guard.free(addr, pages);
  }
}

impl<const BUFFER_SIZE: usize> PageAllocator for DynamicBufferedPageAllocator<BUFFER_SIZE> {
  /// See `PageAllocator::alloc`.
  fn alloc(&mut self) -> Option<usize> {
    // Attempt to refill the page buffer.
    if self.buffer_count == 0 {
      let allocators = unsafe { ptr::addr_of_mut!(ZONE_ALLOCATORS).as_mut().unwrap() };
      let alloc = allocators[LINEAR_MEMORY_ALLOCATOR].as_mut().unwrap();
      let mut guard = alloc.lock();

      while self.buffer_count < BUFFER_SIZE {
        let addr = guard.allocate(1);

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

impl<const BUFFER_SIZE: usize> BlockAllocator for DynamicBufferedPageAllocator<BUFFER_SIZE> {
  /// See `PageAllocator::contiguous_alloc`.
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

impl<const BUFFER_SIZE: usize> Drop for DynamicBufferedPageAllocator<BUFFER_SIZE> {
  /// Release all buffered pages.
  fn drop(&mut self) {
    let allocators = unsafe { ptr::addr_of_mut!(ZONE_ALLOCATORS).as_mut().unwrap() };
    let alloc = allocators[LINEAR_MEMORY_ALLOCATOR].as_mut().unwrap();
    let mut guard = alloc.lock();

    for i in 0..self.buffer_count {
      guard.free(self.page_buffer[i], 1);
    }
  }
}

/// Initialize the memory management module.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
pub fn init() {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  init_allocators();
}

/// Get the per-core linear allocator for the current core.
pub fn get_current_core_linear_allocator() -> &'static mut PerCoreLinearAllocator {
  let core = arch::get_current_core_index();
  unsafe { &mut ptr::addr_of_mut!(PER_CORE_ALLOCATORS).as_mut().unwrap()[core] }
}

/// Initialize the allocators.
fn init_allocators() {
  const ZONE_INFO_INITIALIZER: ZoneInfo = ZoneInfo {
    range: MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: 0,
      size: 0,
    },
    meta_size: 0,
    zone_index: 0,
    start_index: 0,
    end_index: 0,
  };

  // Scan the system memory configuration and aggregate the physical memory
  // ranges into the zones.
  let mem_config = arch::get_device_tree().get_memory_config();
  let mut zone_info: [ZoneInfo; ZONE_ALLOCATOR_COUNT] =
    [ZONE_INFO_INITIALIZER; ZONE_ALLOCATOR_COUNT];
  init_zone_info(&mut zone_info, mem_config);

  // Compute the total metadata size and find a base address in a linear memory
  // range for the metadata.
  let allocators = unsafe { ptr::addr_of_mut!(ZONE_ALLOCATORS).as_mut().unwrap() };
  let alloc_config = unsafe {
    ptr::addr_of_mut!(ZONE_ALLOCATOR_MEMORY_CONFIG)
      .as_mut()
      .unwrap()
  };
  let meta_base = init_allocator_memory_config(alloc_config, mem_config, &zone_info);

  // The metadata for all zones is guaranteed to be in linear memory, so we can
  // use a linear mapping to the metadata.
  let mut curr_meta_base = arch::get_kernel_virtual_base() + meta_base;

  // Construct the allocators.
  for zone in zone_info {
    if zone.range.tag == MemoryZone::InvalidZone {
      continue;
    }

    allocators[zone.zone_index] = Some(SpinLock::new(
      BuddyPageAllocator::new(
        zone.range.base,
        zone.range.size,
        curr_meta_base as *mut u8,
        &alloc_config.get_ranges()[zone.start_index..=zone.end_index],
      )
      .unwrap(),
    ));

    curr_meta_base += zone.meta_size;
  }
}

/// Scan the system memory configuration and fill out the zone info.
///
/// # Parameters
///
/// * `zone_info` - The zone information.
/// * `mem_config` - The system memory configuration.
///
/// # Description
///
/// Finds the bounding range and metadata size for the linear and high memory
/// zones.
fn init_zone_info(zone_info: &mut [ZoneInfo; ZONE_ALLOCATOR_COUNT], mem_config: &MemoryConfig) {
  // Find the bounding range for each zone along with the zone and memory
  // configuration indices.
  for (index, range) in mem_config.get_ranges().iter().enumerate() {
    let zone_index = get_zone_index(range.tag).unwrap();
    let info = &mut zone_info[zone_index];

    if info.range.tag == MemoryZone::InvalidZone {
      info.range = *range;
      info.zone_index = zone_index;
      info.start_index = index;
      info.end_index = index;
    } else if info.range.tag == range.tag {
      let end = range.base + (range.size - 1);
      info.range.size = end - info.range.base + 1;
      info.end_index = index;
    }
  }

  let page_size = arch::get_page_size();

  // Run through the zones and calculate the metadata size rounded up to the
  // nearest page for each bounding range.
  for zone in zone_info {
    zone.meta_size =
      bits::align_up(BuddyPageAllocator::calc_metadata_size(zone.range.size), page_size);
  }
}

/// Initialize the allocator memory configuration.
///
/// # Parameters
///
/// * `alloc_config` - The allocator memory configuration.
/// * `mem_config` - The system memory configuration.
/// * `zone_info` - The zone information.
///
/// # Description
///
/// Calculates the total metadata size required for all zones, then finds a
/// linear memory range large enough to accommodate the metadata. The allocator
/// memory configuration is then updated to exclude metadata from the end of the
/// linear memory range.
///
/// # Returns
///
/// The metadata base address.
fn init_allocator_memory_config(
  alloc_config: &mut MemoryConfig,
  mem_config: &MemoryConfig,
  zone_info: &[ZoneInfo; ZONE_ALLOCATOR_COUNT],
) -> usize {
  *alloc_config = *mem_config;

  let page_size = arch::get_page_size();
  let meta_total = zone_info.iter().fold(0, |acc, z| acc + z.meta_size);
  let mut meta_base = 0;

  for range in alloc_config.get_ranges() {
    match range.tag {
      MemoryZone::LinearMemoryZone => {
        if meta_total < range.size {
          let end = range.base + (range.size - 1);
          meta_base = end - meta_total + 1;
          break;
        }
      }
      _ => {}
    }
  }

  assert_ne!(meta_base, 0);

  let excl = MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: meta_base,
    size: meta_total,
  };

  alloc_config.exclude_range(&excl);

  meta_base
}

/// Get the allocator index for a zone.
///
/// # Parameters
///
/// * `zone` - The memory zone of interest.
///
/// # Returns
///
/// The allocator index, or None if the zone is invalid.
fn get_zone_index(zone: MemoryZone) -> Option<usize> {
  match zone {
    MemoryZone::LinearMemoryZone => Some(LINEAR_MEMORY_ALLOCATOR),
    MemoryZone::HighMemoryZone => Some(HIGH_MEMORY_ALLOCATOR),
    _ => None,
  }
}

/// Run the memory management tests.
#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut crate::test::TestContext) {
  page_allocator::run_tests(context);
}
