//! Buddy Page Allocator

#[cfg(feature = "module_tests")]
mod tests;

use crate::arch;
use crate::arch::memory::MemoryRange;
use crate::support::bits;
use crate::task::Task;
#[cfg(feature = "module_tests")]
use crate::test;
use core::{cmp, ptr, slice};

/// Support blocks that are up to Page Size * 2^10 bytes. For example, with a
/// 4 KiB page size, the largest block size is 4 MiB.
const BLOCK_LEVELS: usize = 11;

/// Linked-list node placed at the beginning of each unallocated block.
#[repr(C)]
struct BlockNode {
  next: usize,
  prev: usize,
  checksum: usize,
}

impl BlockNode {
  /// Construct a new block node with a checksum.
  ///
  /// # Parameters
  ///
  /// * `next` - The physical address of the next node.
  /// * `prev` - The physical address of the previous node.
  ///
  /// # Returns
  ///
  /// A new node.
  fn new(next: usize, prev: usize) -> Self {
    Self {
      next,
      prev,
      checksum: bits::xor_checksum(&[next, prev]),
    }
  }

  /// Verify a node's checksum.
  ///
  /// # Returns
  ///
  /// True if the checksum is valid, false otherwise.
  fn verify_checksum(&self) -> bool {
    bits::xor_checksum(&[self.next, self.prev]) == self.checksum
  }
}

/// Block level metadata
#[derive(Default)]
struct BlockLevel {
  head: usize,
  offset: usize,
}

/// The Buddy Allocator
///
/// https://en.wikipedia.org/wiki/Buddy_memory_allocation
/// https://www.kernel.org/doc/gorman/html/understand/understand009.html
///
///   NOTE: The allocator is NOT thread-safe.
///   NOTE: The allocator does NOT protect against double-free bugs/attacks.
pub struct BuddyPageAllocator<'memory> {
  base: usize,
  size: usize,
  levels: [BlockLevel; BLOCK_LEVELS],
  flags: &'memory mut [usize],
}

impl<'memory> BuddyPageAllocator<'memory> {
  /// Calculate the amount of memory required for the allocator's metadata.
  ///
  /// # Parameters
  ///
  /// * `size` - The size of the memory area in bytes.
  ///
  /// # Description
  ///
  /// If the allocator is going to manage multiple discontinuous ranges of
  /// memory, `size` must be the total size including unused areas between the
  /// ranges. For example, if the allocator will serve the ranges [a:b], [c:d],
  /// and [e:f] where b < c and d < e, the size must be the size of [a:f].
  ///
  /// Because allocators are mutually exclusive, overlapping metadata areas is
  /// allowed. For example, a second allocator serving the ranges (b:c) and
  /// (d:e) may overlap its metadata with the first allocator as neither will
  /// modify the other's state flags.
  ///
  ///                  Bits Representing Ranges:
  ///   +----------+----------+----------+----------+----------+
  ///   |   a:b    |   b:c    |   c:d    |   d:e    |   e:f    |
  ///   +----------+----------+----------+----------+----------+
  ///   ^          ^
  ///   |          `-- Base pointer for Allocator 2's metadata covering [b:e].
  ///   |
  ///   `-- Base point for Allocator 1's metadata covering [a:f].
  ///
  /// # Returns
  ///
  /// The size of the metadata area in bytes.
  pub fn calc_metadata_size(size: usize) -> usize {
    let (mut blocks, mut offset) = Self::calc_first_level(size);

    for _ in 0..BLOCK_LEVELS {
      (blocks, offset) = Self::calc_next_level(blocks, offset);
    }

    offset << bits::WORD_SHIFT
  }

  /// Construct the block level metadata for an allocator.
  ///
  /// # Parameters
  ///
  /// * `size` - The size of the memory area served by the allocator.
  ///
  /// # Returns
  ///
  /// A tuple with the block level metadata structure and the size of the
  /// metadata area in bytes.
  fn make_levels(size: usize) -> ([BlockLevel; BLOCK_LEVELS], usize) {
    let mut levels: [BlockLevel; BLOCK_LEVELS] = Default::default();
    let (mut blocks, mut offset) = Self::calc_first_level(size);

    for level in &mut levels {
      level.offset = offset;
      (blocks, offset) = Self::calc_next_level(blocks, offset);
    }

    (levels, offset << bits::WORD_SHIFT)
  }

  /// Calculate the first block level given a range size.
  ///
  /// # Parameters
  ///
  /// * `size` - The range size in bytes.
  ///
  /// # Returns
  ///
  /// A tuple with the number of blocks at the first level and the initial
  /// offset.
  fn calc_first_level(size: usize) -> (usize, usize) {
    (size >> arch::get_page_shift(), 0)
  }

  /// Calculate the next block level.
  ///
  /// # Parameters
  ///
  /// * `blocks` - The number of blocks at the current level.
  /// * `offset` - The current level's offset.
  ///
  /// # Returns
  ///
  /// A tuple with the number of blocks at the next level and the next level's
  /// offset.
  fn calc_next_level(blocks: usize, offset: usize) -> (usize, usize) {
    // One bit per block pair.
    let bits = (blocks + 1) >> 1;

    // Shift the block count down, and round up the offset to the next word
    // after this level's bits.
    (blocks >> 1, offset + ((bits + bits::WORD_BITS - 1) >> bits::WORD_BIT_SHIFT))
  }

  /// Get a reference to a block's linked-list node.
  ///
  /// # Parameters
  ///
  /// * `addr` - Physical address of the block.
  ///
  /// # Description
  ///
  /// Verifies that the pointer is page-aligned and that the node's checksum is
  /// correct.
  ///
  ///   NOTE: `get_block_node()` and `unget_block_node()` are wrappers for
  ///         `Task::map_page()` and `Task::unmap_page()`. Calls must follow the
  ///         same stack semantics.
  ///
  /// # Returns
  ///
  /// A node reference.
  fn get_block_node(addr: usize) -> &'static BlockNode {
    Self::get_block_node_mut(addr)
  }

  /// Get a mutable reference to a block's linked-list node.
  ///
  /// # Parameters
  ///
  /// * `addr` - Physical address of the block.
  ///
  /// # Description
  ///
  /// Verifies that the pointer is page-aligned and that the node's checksum is
  /// correct.
  ///
  ///   NOTE: `get_block_node_mut()` and `unget_block_node()` are wrappers for
  ///         `Task::map_page()` and `Task::unmap_page()`. Calls must follow the
  ///         same stack semantics.
  ///
  /// # Returns
  ///
  /// A mutable node reference.
  fn get_block_node_mut(addr: usize) -> &'static mut BlockNode {
    let node = Self::get_block_node_unchecked_mut(addr);
    assert!(node.verify_checksum());
    node
  }

  /// Get an unchecked, mutable reference to a block's linked-list node.
  ///
  /// # Parameters
  ///
  /// * `addr` - Physical address of the block.
  ///
  /// # Description
  ///
  /// Verifies that the pointer is page-aligned, but does not verify the check-
  /// sum. Used when the node is not expected to be initialized.
  ///
  ///   NOTE: `get_block_node_unchecked_mut()` and `unget_block_node()` are
  ///         wrappers for `Task::map_page()` and `Task::unmap_page()`. Calls
  ///         must follow the same stack semantics.
  ///
  /// # Returns
  ///
  /// A mutable node reference assumed to be uninitialized.
  fn get_block_node_unchecked_mut(addr: usize) -> &'static mut BlockNode {
    let page_size = arch::get_page_size();
    assert_eq!(bits::align_down(addr, page_size), addr);

    let page = Task::get_current_task_mut().map_page(addr);
    unsafe { &mut *(page as *mut BlockNode) }
  }

  /// Release a block node.
  ///
  /// # Description
  ///
  ///   NOTE: `get_block_node_*()` and `unget_block_node()` are wrappers for
  ///         `Task::map_page()` and `Task::unmap_page()`. Calls must follow the
  ///         same stack semantics.
  fn unget_block_node() {
    Task::get_current_task_mut().unmap_page();
  }

  /// Construct a new page allocator for a given contiguous memory area.
  ///
  /// # Parameters
  ///
  /// * `base` - Base physical address of the memory area served.
  /// * `size` - Size of the memory area.
  /// * `metadata` - A memory block available for metadata.
  /// * `avail` - Available physical address regions with the memory area.
  ///
  /// # Description
  ///
  /// The list of available regions should exclude any regions within the memory
  /// area that the allocator should not use. If the memory reserved for the
  /// allocator's metadata is within the memory area, it too should be excluded
  /// from the available regions.
  ///
  /// # Assumptions
  ///
  /// Assumes that the caller has previously called `calc_metadata_size()` and
  /// verified that the memory pointed to by `metadata` is large enough.
  ///
  /// Assumes that the region ❬base, size❭ is the maximum available range. If
  /// `base` is not page-aligned, it will be aligned up and the size reduced
  /// accordingly. If `size` is not an integer multiple of the page size, it
  /// will be reduced to an integer multiple.
  ///
  /// Assumes the available regions do not overlap.
  ///
  /// # Returns
  ///
  /// A new allocator, or None if:
  ///
  /// * `base` is 0 after alignment.
  /// * `base` is not a valid physical address.
  /// * `size` is less than the page size after alignment.
  /// * `base + size` would overflow a pointer after alignment.
  /// * `metadata` is null.
  /// * `avail` is empty.
  pub fn new(base: usize, size: usize, metadata: *mut u8, avail: &[MemoryRange]) -> Option<Self> {
    let page_size = arch::get_page_size();
    let max_physical = arch::get_maximum_physical_address();

    // Sanity check the inputs so that we can calculate an initial end address.
    if base > max_physical {
      return None;
    }

    if max_physical - base < (size - 1) {
      return None;
    }

    let end = base + size - 1;

    // Now update the base address for page-alignment.
    let base = bits::align_up(base, page_size);

    // Now update the new size for page-alignment.
    let size = bits::align_down(end - base + 1, page_size);

    // At least one page is required.
    if size < page_size {
      return None;
    }

    // A metadata area is required.
    if metadata == ptr::null_mut() {
      return None;
    }

    // At least one range must be available.
    if avail.is_empty() {
      return None;
    }

    // Validate all ranges are within the aligned memory area. Allow integer
    // overflow to assert.
    for range in avail {
      let range_end = range.base + (range.size - 1);

      if range.base < base || range_end > end {
        return None;
      }
    }

    // Make the allocator.
    let (levels, meta_size) = Self::make_levels(size);

    let mut allocator = Self {
      base,
      size,
      levels,
      flags: unsafe {
        slice::from_raw_parts_mut(metadata as *mut usize, meta_size >> bits::WORD_SHIFT)
      },
    };

    allocator.init_metadata(&avail);

    Some(allocator)
  }

  /// Attempts to allocate a contiguous block of pages.
  ///
  /// # Parameters
  ///
  /// * `pages` - The requested number of pages.
  ///
  /// # Description
  ///
  /// If `pages` is not a power of 2, the size of the block returned will be the
  /// smallest power of 2 pages larger than the requested number of pages.
  ///
  /// # Returns
  ///
  /// A tuple with the base physical address of the contiguous block and the
  /// actual number of pages allocated, or None if the allocator could not find
  /// an available contiguous block of the requested size.
  pub fn allocate(&mut self, pages: usize) -> Option<(usize, usize)> {
    if pages == 0 {
      return None;
    }

    // Calculate the level with the minimum block size.
    let min_level = bits::ceil_log2(pages);

    for level in min_level..BLOCK_LEVELS {
      if self.levels[level].head == 0 {
        continue;
      }

      let block = self.split_free_block(level, min_level);
      let pages = 1 << min_level;
      return Some((block, pages));
    }

    // No blocks available.
    None
  }

  /// Frees a block of memory.
  ///
  /// # Parameters
  ///
  /// * `base` - The base physical address of the block.
  /// * `pages` - The number of pages in the block.
  ///
  /// # Description
  ///
  /// The number of pages must be a power of 2. The base address of the block
  /// must be aligned on an address that is a multiple of the block size. The
  /// function ignores a base address of 0 or a page count of 0.
  pub fn free(&mut self, base: usize, pages: usize) {
    if (base == 0) || (pages == 0) {
      return;
    }

    assert!(bits::is_power_of_2(pages));

    let min_level = bits::floor_log2(pages);
    assert!(min_level < BLOCK_LEVELS);
    assert_eq!(base & (pages - 1), 0);

    let page_shift = arch::get_page_shift();
    let range_end = base + ((pages << page_shift) - 1);
    let alloc_end = self.base + (self.size - 1);
    assert!(base >= self.base && range_end <= alloc_end);

    let mut base = base;

    for level in min_level..BLOCK_LEVELS {
      let (index, bit_idx) = self.get_flag_index_and_bit(base, level);

      // The allocator does not protect against double-free, so the assumption
      // here is that the buddy block is in use if the bit is zero, and we
      // cannot coalesce the two.
      if self.flags[index] & (1 << bit_idx) == 0 {
        self.add_to_list(level, base);
        break;
      }

      // If the bit is not zero, get the buddy block address using XOR. Remove
      // the buddy from the list at this level, then update the base address to
      // the minimum of the two.
      //
      //   NOTE: The buddy address is calculated relative to the beginning of
      //         the allocator's memory region.
      let buddy_addr = ((base - self.base) ^ ((1 << level) << page_shift)) + self.base;
      self.remove_from_list(level, buddy_addr);
      base = cmp::min(base, buddy_addr);
    }
  }

  /// Initializes the allocator's linked list and accounting metadata.
  ///
  /// # Parameters
  ///
  /// * `avail` - Available physical regions with the memory area.
  ///
  /// # Assumptions
  ///
  /// The available regions have already been validated by the caller.
  fn init_metadata(&mut self, avail: &[MemoryRange]) {
    let page_shift = arch::get_page_shift();
    let page_size = arch::get_page_size();

    self.flags.fill(0);

    for range in avail {
      let mut addr = range.base;
      let mut remaining = range.size;

      while remaining >= page_size {
        // Consider the address 0x1ed000. With 4 KiB pages, this address is
        // 0x1ed pages from the beginning of the address space. Each block must
        // be exactly aligned on a multiple of its size. We can figure out the
        // alignment using the least-significant 1 bit in the block number. For
        // example, 0x1ed = 0b111101101. The least-significant 1 bit is bit 0,
        // so the address is aligned on a 1-page multiple, and we cannot
        // allocate more than a single page at that address.
        //
        // After making a single page block available at 0x1ed000, we increment
        // the address to 0x1ee000. This is block 0x1ee = 0b111101110. This
        // address is aligned on a 2-page multiple. So, we make a 2-page block
        // available and increment the address to 0x1f0000. This address is
        // aligned on a 16-page multiple, so the next address is 0x200000. This
        // address is aligned on a 512-page multiple, and so on.
        //
        // Page 0 should never be used.
        let page_num = addr >> page_shift;
        let addr_align = bits::least_significant_bit(page_num);
        let max_level = cmp::min(bits::floor_log2(addr_align), BLOCK_LEVELS - 1);

        // Of course, the above is only half the story. We also have to cap the
        // maximum block size by the remaining memory size.
        let pages_remaining = remaining >> page_shift;
        let level = cmp::min(bits::floor_log2(pages_remaining), max_level);
        let blocks = 1 << level;
        let size = blocks << page_shift;

        // Add the block to the level's available list.
        self.add_to_list(level, addr);

        addr += size;
        remaining -= size;
      }
    }
  }

  /// Get the flag index and bit for a given physical address at a given level.
  ///
  /// # Parameters
  ///
  /// * `block_addr` - The physical block address.
  /// * `level` - The block level.
  ///
  /// # Assumptions
  ///
  /// Assumes that the start address for the block is aligned on a multiple of
  /// the block size for the specified level.
  ///
  /// # Returns
  ///
  /// A tuple with the absolute word index into the metadata flags and the bit
  /// index in that word for the block.
  fn get_flag_index_and_bit(&self, block_addr: usize, level: usize) -> (usize, usize) {
    let page_shift = arch::get_page_shift();
    let page_num = (block_addr - self.base) >> page_shift;
    let block_num = page_num >> level;
    let block_pair = block_num >> 1;
    let index = self.levels[level].offset + (block_pair >> bits::WORD_BIT_SHIFT);
    let bit = block_pair & bits::WORD_BIT_MASK;

    (index, bit)
  }

  /// Split a free block until it is the required size.
  ///
  /// # Parameters
  ///
  /// * `level` - The level at which to split.
  /// * `min_level` - The level at which the split stops.
  ///
  /// # Description
  ///
  /// Assumes at least one block is available at `level`. Removes the first
  /// available block, splits it in half, and adds the odd half to the first
  /// list at `level - 1`. Repeats until reaching `min_level`.
  ///
  /// # Returns
  ///
  /// The block address of the block removed from `level`.
  fn split_free_block(&mut self, level: usize, min_level: usize) -> usize {
    let page_size = arch::get_page_size();
    let block_addr = self.pop_from_list(level);

    // For this example, just assume 1 byte pages starting at 0 for simplicity.
    //
    // Assume block 2 is free at level 4 covering pages [32, 48), and assume we
    // want to allocate two pages. Remove 0x20 from block 4. At level 3, the odd
    // buddy is 0x20 | 0x08:
    //
    //  0x20                             0x28                             0x30
    //   +--------+--------+----------------+--------------------------------+
    //   |                                  |                                |
    //   +--------+--------+----------------+--------------------------------+
    //
    // Add 0x28 to the free list at level 3 to cover pages [40, 48), then move
    // down. At level 2, the odd buddy is 0x20 | 0x04:
    //
    //  0x20            0x24             0x28
    //   +--------+--------+----------------+----
    //   |                 |                |
    //   +--------+--------+----------------+----
    //
    // Add 0x24 to the free list at level 2 to cover pages [36, 40), then move
    // down. At level 1, the odd buddy is 0x20 | 0x02:
    //
    //  0x20   0x22     0x24
    //   +--------+--------+----
    //   |        |        |
    //   +--------+--------+----
    //
    // Add 0x22 to the free list at level 1 to cover pages [34, 36). We are now
    // done splitting and can return 0x20 as the two-page block covering pages
    // [32, 34).
    for l in (min_level..level).rev() {
      let buddy_addr = block_addr | (page_size << l);
      self.add_to_list(l, buddy_addr);
    }

    block_addr
  }

  /// Adds a block to the tail of a level's list of available blocks.
  ///
  /// # Parameters
  ///
  /// * `level` - The level to which the block will be added.
  /// * `block_addr` - The virtual block address to add to the list.
  fn add_to_list(&mut self, level: usize, block_addr: usize) {
    let (index, bit_idx) = self.get_flag_index_and_bit(block_addr, level);
    let head_addr = self.levels[level].head;
    let block = Self::get_block_node_unchecked_mut(block_addr);

    // If the list is empty, initialize a new node that points only to itself
    // and return the block address as the new head address. Otherwise, add the
    // block to the tail of the list.
    if head_addr == 0 {
      *block = BlockNode::new(block_addr, block_addr);
      self.levels[level].head = block_addr;
    } else {
      let head = Self::get_block_node_mut(head_addr);
      let prev = Self::get_block_node_mut(head.prev);

      *block = BlockNode::new(head_addr, head.prev);
      *head = BlockNode::new(head.next, block_addr);
      *prev = BlockNode::new(block_addr, prev.prev);

      Self::unget_block_node();
      Self::unget_block_node();
    }

    Self::unget_block_node();

    self.flags[index] ^= 1 << bit_idx;
  }

  /// Pop the head of a level's free list.
  ///
  /// # Parameters
  ///
  /// * `level` - The level from which to remove a free block.
  ///
  /// # Description
  ///
  /// Assumes that the list is not empty.
  ///
  /// # Returns
  ///
  /// The block address popped from the list.
  fn pop_from_list(&mut self, level: usize) -> usize {
    let head_addr = self.levels[level].head;
    self.remove_from_list(level, head_addr);
    head_addr
  }

  /// Removes a specific block from a level's free list.
  ///
  /// # Parameters
  ///
  /// * `level` - The level from which to remove a free block.
  /// * `block_addr` - The virtual block address to remove from the list.
  fn remove_from_list(&mut self, level: usize, block_addr: usize) {
    let (index, bit_idx) = self.get_flag_index_and_bit(block_addr, level);
    let head_addr = self.levels[level].head;
    let block = Self::get_block_node(block_addr);

    // If the block points to itself, sanity check the block and list, then
    // set the head to zero. Otherwise, remove the block.
    if block.next == block_addr {
      assert_eq!(block.prev, block.next);
      assert_eq!(head_addr, block_addr);
      self.levels[level].head = 0;
    } else {
      let prev = Self::get_block_node_mut(block.prev);
      let next = Self::get_block_node_mut(block.next);

      *prev = BlockNode::new(block.next, prev.prev);
      *next = BlockNode::new(next.next, block.prev);

      Self::unget_block_node();
      Self::unget_block_node();

      // If this block is the head block, move the head to the next block.
      if block_addr == head_addr {
        self.levels[level].head = block.next;
      }
    }

    Self::unget_block_node();

    self.flags[index] ^= 1 << bit_idx;
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut test::TestContext) {
  tests::run_tests(context);
}
