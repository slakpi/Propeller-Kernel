//! Memory Management

pub mod table_allocator;

/// Mapping strategies to use when mapping blocks of memory.
pub enum MappingStrategy {
  /// A strategy that uses architecture-specific techniques, such as ARM
  /// sections, to map a block of memory using the fewest table entries.
  Compact,
  /// A strategy that maps a block of memory to individual pages.
  Granular,
}
