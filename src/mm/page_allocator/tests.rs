//! Buddy Page Allocator Tests

use super::{BlockLevel, BuddyPageAllocator};
use crate::arch;
use crate::arch::memory::{MemoryConfig, MemoryRange, MemoryZone};
use crate::support::bits;
use crate::{check_eq, check_neq, check_none, check_not_none, execute_test, mark_fail, test};
use core::{iter, ptr, slice};

/// Test with 4 KiB pages.
const TEST_PAGE_SIZE: usize = 4096;
const TEST_PAGE_SHIFT: usize = 12;

/// Test with 2047 pages. The non-power of 2 tests proper setup and accounting.
const TEST_PAGE_COUNT: usize = 2047;
const TEST_MEM_SIZE: usize = TEST_PAGE_SIZE * TEST_PAGE_COUNT;

/// Make the memory buffer larger to accommodate testing offset blocks.
const TEST_BUFFER_SIZE: usize = TEST_MEM_SIZE + (TEST_PAGE_SIZE * 256);

/// Each flag bit represents a pair of blocks. The number of blocks in a level
/// is `floor( pages / block size )`. The number of bits required at each level
/// is `ceil( blocks / 2 )`.
///
/// The metadata has to cover TEST_BUFFERS_SIZE so that a region of
/// TEST_MEM_SIZE bytes can be placed with an offset.
///
/// Block Size (Pages)       Offset (Words)       Flag Bits
///                          32-bit  64-bit
/// -------------------------------------------------------
///    1                       0       0               1152
///    2                      36      18                576
///    4                      54      27                288
///    8                      63      32                144
///   16                      68      35                 72
///   32                      71      37                 36
///   64                      73      38                 18
///  128                      74      39                  9
///  256                      75      40                  4
///  512                      76      41                  2
/// 1024                      77      42                  1
/// -------------------------------------------------------
///                           78      43
#[cfg(target_pointer_width = "32")]
const EXPECTED_METADATA_SIZE: usize = 78 << bits::WORD_SHIFT;
#[cfg(target_pointer_width = "64")]
const EXPECTED_METADATA_SIZE: usize = 43 << bits::WORD_SHIFT;

/// The total size of the test memory buffer.
const TOTAL_MEM_SIZE: usize = TEST_BUFFER_SIZE + EXPECTED_METADATA_SIZE;

/// The allocator should serve up blocks of 2^0 up to 2^10 pages.
const EXPECTED_BLOCK_LEVELS: usize = 11;

/// Alignment type.
#[repr(align(0x400000))]
struct _Align4MiB;

/// Wrapper type to align the memory block. Aligning to 4 MiB allows the tests
/// to control how the allocator arranges blocks without needing to know the
/// kernel size.
struct _MemWrapper {
  _alignment: [_Align4MiB; 0],
  mem: [u8; TOTAL_MEM_SIZE],
}

/// Use a statically allocated memory block within the kernel to avoid any
/// issues with memory configuration.
static mut TEST_MEM: _MemWrapper = _MemWrapper {
  _alignment: [],
  mem: [0xcc; TOTAL_MEM_SIZE],
};

/// Test memory configuration.
///
///   NOTE: This is static to save stack space.
static mut TEST_MEM_CONFIG: MemoryConfig = MemoryConfig::new(MemoryZone::InvalidZone);

/// Represents an allocator state usings lists of block addresses.
struct AllocatorState<'a> {
  levels: [&'a [usize]; EXPECTED_BLOCK_LEVELS],
}

/// Test entry-point.
pub fn run_tests(context: &mut test::TestContext) {
  execute_test!(context, test_size_calculation);
  execute_test!(context, test_level_construction);
  execute_test!(context, test_metadata_front_load);
  execute_test!(context, test_metadata_end_load);
  execute_test!(context, test_available_regions);
  execute_test!(context, test_construction_errors);
  execute_test!(context, test_allocation);
  execute_test!(context, test_free);
}

/// Test calculating the size required for the allocator metadata.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_size_calculation(context: &mut test::TestContext) {
  let size = BuddyPageAllocator::calc_metadata_size(TEST_BUFFER_SIZE);
  check_eq!(context, size, EXPECTED_METADATA_SIZE);

  let size = BuddyPageAllocator::calc_metadata_size(0);
  check_eq!(context, size, 0);
}

/// Test initializing the head pointers and bit array offsets.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_level_construction(context: &mut test::TestContext) {
  let exp_levels = make_expected_levels();

  let (levels, size) = BuddyPageAllocator::make_levels(TEST_BUFFER_SIZE);
  check_eq!(context, size, EXPECTED_METADATA_SIZE);
  check_eq!(context, levels.len(), exp_levels.len());

  for (a, b) in iter::zip(levels, exp_levels) {
    check_eq!(context, a.head, b.head);
    check_eq!(context, a.offset, b.offset);
  }
}

/// Test front-loading free blocks.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// The test starts by offseting the base address of the memory area by one
/// page. When shifting the address down by TEST_PAGE_SHIFT, the least
/// significant 1 bit in the page number will be bit 0 meaning the largest block
/// that can be allocated at that address is a single page.
///
/// The allocator should make a single block available at level 0, then add one
/// page to the base address. Now the least significant 1 bit in the page number
/// is bit 1 meaning the largest block that can be allocated at that address is
/// two pages.
///
/// The process should repeat placing a single block at the lowest address for
/// each level.
///
/// Note that the test expects the block number at each level to be block 2, the
/// odd buddy to the first block at each level (the block numbers are 1-based).
///
///          Base Address
///      +---+---+---
///   0  | / | 2 |
///      +---+---+---
///
///      +-------+-------+---
///   1  | / / / |   2   |
///      +-------+-------+---
///
///      +---------------+---------------+---
///   2  | / / / / / / / |       2       |
///      +---------------+---------------+---
///
///   ...etc...
fn test_metadata_front_load(context: &mut test::TestContext) {
  let mut allocator = make_allocator(TEST_PAGE_SIZE);
  let (base_addr, _) = get_addrs();

  verify_allocator(
    context,
    &allocator,
    &AllocatorState {
      levels: [
        &[make_block_addr(base_addr, 2, 0)],
        &[make_block_addr(base_addr, 2, 1)],
        &[make_block_addr(base_addr, 2, 2)],
        &[make_block_addr(base_addr, 2, 3)],
        &[make_block_addr(base_addr, 2, 4)],
        &[make_block_addr(base_addr, 2, 5)],
        &[make_block_addr(base_addr, 2, 6)],
        &[make_block_addr(base_addr, 2, 7)],
        &[make_block_addr(base_addr, 2, 8)],
        &[make_block_addr(base_addr, 2, 9)],
        &[make_block_addr(base_addr, 2, 10)],
      ],
    },
  );
}

/// Test end-loading free blocks.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// The test starts with the base address of the memory area aligned to allow
/// the largest possible block. The size of the memory area is set to
/// `(2^11) - 1`. This allows exactly one block at each level.
///
/// Because the alignment allows the allocator to place large blocks first, each
/// block should be placed at the highest address for each level.
///
/// Note that the test expects the block number at each level to be an even
/// buddy (the block numbers are 1-based).
///
///      Base Address
///      +-------------------------------+---
///   10 |               1               |
///      +-------------------------------+---
///
///      +-------------------------------+---------------+---
///    9 | / / / / / / / / / / / / / / / |       3       |
///      +-------------------------------+---------------+---
///
///      +-------------------------------+---------------+-------+---
///    8 | / / / / / / / / / / / / / / / | / / / / / / / |   7   |
///      +-------------------------------+---------------+-------+---
///
///  ...etc...
fn test_metadata_end_load(context: &mut test::TestContext) {
  let mut allocator = make_allocator(0);
  let (base_addr, _) = get_addrs();

  verify_allocator(
    context,
    &allocator,
    &AllocatorState {
      levels: [
        &[make_block_addr(base_addr, 2047, 0)],
        &[make_block_addr(base_addr, 1023, 1)],
        &[make_block_addr(base_addr, 511, 2)],
        &[make_block_addr(base_addr, 255, 3)],
        &[make_block_addr(base_addr, 127, 4)],
        &[make_block_addr(base_addr, 63, 5)],
        &[make_block_addr(base_addr, 31, 6)],
        &[make_block_addr(base_addr, 15, 7)],
        &[make_block_addr(base_addr, 7, 8)],
        &[make_block_addr(base_addr, 3, 9)],
        &[make_block_addr(base_addr, 1, 10)],
      ],
    },
  )
}

/// Test front- and end-loading using disjoint holes in the memory area.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_available_regions(context: &mut test::TestContext) {
  let (base_addr, meta_addr) = get_addrs();
  let virt_base = arch::get_kernel_virtual_base();
  let meta = meta_addr as *mut u8;

  // Set up the available memory to have two holes:
  //
  //     Pages
  //     1     511       512           1023
  //   +---+---------+---------+------------------+
  //   | / |         | / / / / |                  |
  //   +---+---------+---------+------------------+
  //
  // The 1-page hole at the beginning should cause front-loading for the 511-
  // page block and the 512-page hole should cause end-loading for the 1023-
  // page block.
  let avail = &[
    MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: base_addr + TEST_PAGE_SIZE,
      size: 511 * TEST_PAGE_SIZE,
    },
    MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: base_addr + (1024 * TEST_PAGE_SIZE),
      size: TEST_MEM_SIZE - (1024 * TEST_PAGE_SIZE),
    },
  ];

  let allocator = BuddyPageAllocator::new(base_addr, TOTAL_MEM_SIZE, meta, avail);
  check_not_none!(context, allocator);

  verify_allocator(
    context,
    &allocator.unwrap(),
    &AllocatorState {
      levels: [
        &[
          make_block_addr(base_addr, 2, 0),
          make_block_addr(base_addr, 2047, 0),
        ],
        &[
          make_block_addr(base_addr, 2, 1),
          make_block_addr(base_addr, 1023, 1),
        ],
        &[
          make_block_addr(base_addr, 2, 2),
          make_block_addr(base_addr, 511, 2),
        ],
        &[
          make_block_addr(base_addr, 2, 3),
          make_block_addr(base_addr, 255, 3),
        ],
        &[
          make_block_addr(base_addr, 2, 4),
          make_block_addr(base_addr, 127, 4),
        ],
        &[
          make_block_addr(base_addr, 2, 5),
          make_block_addr(base_addr, 63, 5),
        ],
        &[
          make_block_addr(base_addr, 2, 6),
          make_block_addr(base_addr, 31, 6),
        ],
        &[
          make_block_addr(base_addr, 2, 7),
          make_block_addr(base_addr, 15, 7),
        ],
        &[
          make_block_addr(base_addr, 2, 8),
          make_block_addr(base_addr, 7, 8),
        ],
        &[make_block_addr(base_addr, 3, 9)],
        &[],
      ],
    },
  );
}

/// Test that the allocator constructor sanity checks parameters.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_construction_errors(context: &mut test::TestContext) {
  let (base_addr, meta_addr) = get_addrs();
  let meta = meta_addr as *mut u8;

  let good_avail = &[MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: base_addr,
    size: TEST_MEM_SIZE,
  }];

  let bad_avail: &[MemoryRange] = &[];

  // Base case, verify valid parameters produce a valid allocator.
  let allocator = BuddyPageAllocator::new(base_addr, TOTAL_MEM_SIZE, meta, good_avail);
  check_not_none!(context, allocator);

  // Use a base address that aligns down to 0.
  let allocator = BuddyPageAllocator::new(0, TOTAL_MEM_SIZE, meta, good_avail);
  check_none!(context, allocator);

  // Use a memory size that aligns done to a size less than a page.
  let allocator = BuddyPageAllocator::new(base_addr, TEST_PAGE_SIZE - 1, meta, good_avail);
  check_none!(context, allocator);

  // Use a base address and memory size that would overflow a pointer.
  let allocator = BuddyPageAllocator::new(base_addr, usize::MAX, meta, good_avail);
  check_none!(context, allocator);

  // Use a null metadata pointer.
  let allocator = BuddyPageAllocator::new(base_addr, TOTAL_MEM_SIZE, ptr::null_mut(), good_avail);
  check_none!(context, allocator);

  // Use an empty list of available memory regions.
  let allocator = BuddyPageAllocator::new(base_addr, TOTAL_MEM_SIZE, meta, bad_avail);
  check_none!(context, allocator);

  // TODO: Error check providing virtual addresses and invalid available ranges.
}

/// Test allocation.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// For each block size level, the test starts with a cleanly initialized
/// allocator then allocates all blocks at that level. The test verifies that
/// the blocks allocated are on the proper memory boundaries, the block is the
/// correct size, and that there is no overlap. After allocating all blocks at
/// that level, the test verifies that no more blocks can be allocated.
fn test_allocation(context: &mut test::TestContext) {
  for level in 0..EXPECTED_BLOCK_LEVELS {
    let mut pages: [bool; TEST_PAGE_COUNT] = [false; TEST_PAGE_COUNT];
    let exp_count = 1 << level;
    let mask = (TEST_PAGE_SIZE << level) - 1;
    let mut allocator = make_allocator(0);
    let (base_addr, _) = get_addrs();

    for _ in 0..(TEST_PAGE_COUNT >> level) {
      let result = allocator.allocate(exp_count);
      check_not_none!(context, result);

      let (addr, act_count) = result.unwrap();
      check_eq!(context, addr & mask, 0);
      check_eq!(context, act_count, exp_count);

      let start_page = (addr - base_addr) >> TEST_PAGE_SHIFT;
      let end_page = start_page + act_count;
      for i in start_page..end_page {
        check_eq!(context, pages[i], false);
        pages[i] = true;
      }
    }

    let result = allocator.allocate(exp_count);
    check_none!(context, result);
  }
}

/// Test freeing blocks.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// The test starts with an allocator and allocates all available blocks. After
/// allocating all blocks, the test frees each block sequentially from the
/// beginning and verifies blocks coalesce as they are freed.
fn test_free(context: &mut test::TestContext) {
  let mut allocator = make_allocator(0);
  let (base_addr, _) = get_addrs();

  for i in 0..EXPECTED_BLOCK_LEVELS {
    _ = allocator.allocate(1 << i);
  }

  let result = allocator.allocate(1);
  check_none!(context, result);

  let mut mask = 0;
  let mut addr = base_addr;
  for j in 0..TEST_PAGE_COUNT {
    allocator.free(addr, 1);
    mask += 1;
    addr += TEST_PAGE_SIZE;

    for i in 0..EXPECTED_BLOCK_LEVELS {
      let bit = 1 << i;

      if mask & bit == 0 {
        check_eq!(context, allocator.levels[i].head, 0);
      } else {
        check_neq!(context, allocator.levels[i].head, 0);
      }
    }
  }
}

#[cfg(target_pointer_width = "32")]
fn make_expected_levels() -> [BlockLevel; EXPECTED_BLOCK_LEVELS] {
  [
    BlockLevel { head: 0, offset: 0 },
    BlockLevel {
      head: 0,
      offset: 36,
    },
    BlockLevel {
      head: 0,
      offset: 54,
    },
    BlockLevel {
      head: 0,
      offset: 63,
    },
    BlockLevel {
      head: 0,
      offset: 68,
    },
    BlockLevel {
      head: 0,
      offset: 71,
    },
    BlockLevel {
      head: 0,
      offset: 73,
    },
    BlockLevel {
      head: 0,
      offset: 74,
    },
    BlockLevel {
      head: 0,
      offset: 75,
    },
    BlockLevel {
      head: 0,
      offset: 76,
    },
    BlockLevel {
      head: 0,
      offset: 77,
    },
  ]
}

#[cfg(target_pointer_width = "64")]
fn make_expected_levels() -> [BlockLevel; EXPECTED_BLOCK_LEVELS] {
  [
    BlockLevel { head: 0, offset: 0 },
    BlockLevel {
      head: 0,
      offset: 18,
    },
    BlockLevel {
      head: 0,
      offset: 27,
    },
    BlockLevel {
      head: 0,
      offset: 32,
    },
    BlockLevel {
      head: 0,
      offset: 35,
    },
    BlockLevel {
      head: 0,
      offset: 37,
    },
    BlockLevel {
      head: 0,
      offset: 38,
    },
    BlockLevel {
      head: 0,
      offset: 39,
    },
    BlockLevel {
      head: 0,
      offset: 40,
    },
    BlockLevel {
      head: 0,
      offset: 41,
    },
    BlockLevel {
      head: 0,
      offset: 42,
    },
  ]
}

/// Get the memory region and metadata addresses.
///
/// # Description
///
/// The allocator serves the first `TEST_BUFFER_SIZE` bytes in `TEST_MEM::mem`
/// and reserves the final `EXPECTED_METADATA_SIZE` bytes of the buffer for the
/// metadata.
///
/// # Returns
///
/// A tuple with the physical address of test memory region and the virtual
/// address of the metadata.
fn get_addrs() -> (usize, usize) {
  let virt_base = arch::get_kernel_virtual_base();
  let phys_addr =
    unsafe { ptr::addr_of!(TEST_MEM).as_ref().unwrap().mem.as_ptr() as usize } - virt_base;

  (phys_addr, virt_base + phys_addr + TEST_BUFFER_SIZE)
}

/// Get the physical address of a block at a given level.
///
/// # Parameters
///
/// * `base_addr` - The base physical address.
/// * `block` - The 1-based block number.
/// * `level` - The level.
///
/// # Returns
///
/// The block's physical address.
fn make_block_addr(base_addr: usize, block: usize, level: usize) -> usize {
  assert!(block > 0);
  base_addr + ((TEST_PAGE_SIZE << level) * (block - 1))
}

/// Construct a test allocator.
///
/// # Parameters
///
/// * `base_offset` - An offset, in bytes, for the available region.
///
/// # Description
///
/// Constructs a test allocator with a single available region. The region can
/// be offset by up to 256 pages. With a base offset of 8 pages (32 KiB), the
/// allocator layout looks like:
///
///     |------------------ TOTAL_MEM_SIZE ------------------|
///
///     |----------- TEST_BUFFER_SIZE ------------|
///
///                  TEST_MEM_SIZE        248 Pgs
///     +-----+-------------------------+---------+----------+
///     | / / | Available Region        | / / / / | Metadata |
///     +-----+-------------------------+---------+----------+
///     ^     ^
///     |     `- Base Addr + 8 Pages
///     `- Base Addr
///
/// The allocator serves the region ❬Base Addr, TEST_BUFFER_SIZE❭ where the
/// region ❬Base Addr + Base Offset, TEST_MEM_SIZE❭ is available. The metadata
/// starts after the allocator's region.
///
/// # Returns
///
/// The new allocator.
fn make_allocator(base_offset: usize) -> BuddyPageAllocator<'static> {
  let (base_addr, meta_addr) = get_addrs();

  unsafe { ptr::addr_of_mut!(TEST_MEM).as_mut().unwrap().mem.fill(0xcc) };

  let avail = &[MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: base_addr + base_offset,
    size: TEST_MEM_SIZE,
  }];

  // Assume this will never fail. If it does, something is wrong with the test
  // setup.
  BuddyPageAllocator::new(base_addr, TEST_BUFFER_SIZE, meta_addr as *mut u8, avail).unwrap()
}

/// Verifies the state of an allocator.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `allocator` - The allocator to verify.
/// * `state` - The expected allocator state.
fn verify_allocator(
  context: &mut test::TestContext,
  allocator: &BuddyPageAllocator,
  state: &AllocatorState,
) {
  let mut blocks = TEST_MEM_SIZE >> TEST_PAGE_SHIFT;
  let mut level_shift = 0;

  for (level, exp_blocks) in iter::zip(&allocator.levels, &state.levels) {
    if exp_blocks.is_empty() {
      check_eq!(context, level.head, 0);
      continue;
    }

    if level.head == 0 {
      mark_fail!(context, "Head pointer is null.");
      continue;
    }

    let mut ptr = level.head;
    let mut idx = 0;
    let mut mask = 0;

    let bits = (blocks + 1) >> 1;
    let words = (bits + bits::WORD_BITS - 1) >> bits::WORD_BIT_SHIFT;
    blocks >>= 1;

    for block in *exp_blocks {
      let node = BuddyPageAllocator::get_block_node(ptr);
      check_eq!(context, ptr, *block);
      ptr = node.next;

      let page_num = (*block - allocator.base) >> TEST_PAGE_SHIFT;
      let block_num = page_num >> level_shift;
      let block_pair = block_num >> 1;
      let block_idx = block_pair >> bits::WORD_BIT_SHIFT;

      if block_idx > idx {
        for i in idx..block_idx {
          check_eq!(context, allocator.flags[level.offset + i], mask);
          mask = 0;
        }

        idx = block_idx;
      }

      mask ^= 1 << (block_pair & bits::WORD_BIT_MASK);
    }

    for i in idx..words {
      check_eq!(context, allocator.flags[level.offset + i], mask);
      mask = 0;
    }

    check_eq!(context, ptr, exp_blocks[0]);

    for block in exp_blocks.iter().rev() {
      let node = BuddyPageAllocator::get_block_node(ptr);
      ptr = node.prev;
      check_eq!(context, ptr, *block);
    }

    check_eq!(context, ptr, *exp_blocks.first().unwrap());

    level_shift += 1;
  }
}
