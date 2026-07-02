//! Memory Configuration Utilities

use crate::support::{bits, range, range_set};

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

/// Physically-contiguous page block allocator interface.
pub trait PageAllocator {
  const MAX_BLOCK_PAGES: usize;

  /// Allocate a physically-contiguous block of pages from memory.
  ///
  /// # Parameters
  ///
  /// * `pages` - The minimum number of pages to allocate.
  ///
  /// # Description
  ///
  /// The allocated block has the following guarantees:
  ///
  /// * It is physically contiguous.
  /// * The size will be the smallest power-of-2 number of pages equal to or
  ///   larger than the requested number of pages.
  /// * The block will be aligned to the final size.
  ///
  /// # Returns
  ///
  /// A tuple with the physical base address of the block and the actual number
  /// of pages allocated, or None if a block of the requested size could not be
  /// allocated.
  fn alloc(&mut self, pages: usize) -> Option<(usize, usize)>;

  /// Allocate a physicall-contiguous block of pages from memory and zero it.
  ///
  /// # Parameters
  ///
  /// * `pages` - The minimum number of pages to allocate.
  ///
  /// # Description
  ///
  /// See `PageAllocator::alloc()` for the guarantees provided.
  ///
  /// Refer to: https://docs.kernel.org/core-api/memory-allocation.html. This is
  /// likely the best choice for most allocations, but it is most definitely the
  /// right choice when allocating memory that will be given to a user process.
  fn alloc_and_zero(&mut self, pages: usize) -> Option<(usize, usize)>;

  /// Free a contiguous block of pages allocated by this allocator.
  ///
  /// # Parameters
  ///
  /// * `addr` - The physical base address of the block. May be null.
  /// * `pages` - The number of pages to free.
  ///
  /// # Description
  ///
  /// If `addr` is null or `pages` is zero, no action is taken. It is safe to
  /// pass in a dangling pointer if `pages` is zero.
  fn free(&mut self, addr: usize, pages: usize);

  /// Get the amount of memory currently allocated by this allocator in bytes.
  fn get_alloc_mem(&self) -> usize;

  /// Get the amount of memory currently available to this allocator in bytes.
  fn get_free_mem(&self) -> usize;
}
