//! Common Memory Configuration Utilities

use crate::support::{range, range_set};

/// Memory zone tags.
#[derive(Copy, Clone)]
pub enum MemoryZone {
  /// Default for uninitialized allocators.
  InvalidZone,
  /// Low memory zones are fully mapped into the kernel's address space.
  LowMemoryZone,
  /// High memory is only meaningful on 32-bit architectures. Allocating from a
  /// high memory zone is slower than allocating from a low memory zone.
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
