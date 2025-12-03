//! ARM Memory Management

use crate::mm::{MappingStrategy, table_allocator::TableAllocator};
use crate::support::bits;
use core::{cmp, ptr, slice};

unsafe extern "C" {
  fn mmu_update_table_entry_local(
    desc_vaddr: usize,
    virt_addr: usize,
    desc: usize,
    desc_high: usize,
  );
}

const LEVEL_1_TABLE_SHIFT_LONG: usize = 2;
const LEVEL_2_TABLE_SHIFT_LONG: usize = 9;
const LEVEL_3_TABLE_SHIFT_LONG: usize = 9;

const LEVEL_3_SHIFT_LONG: usize = super::get_page_shift();
const LEVEL_2_SHIFT_LONG: usize = LEVEL_3_SHIFT_LONG + LEVEL_3_TABLE_SHIFT_LONG;
const LEVEL_1_SHIFT_LONG: usize = LEVEL_2_SHIFT_LONG + LEVEL_2_TABLE_SHIFT_LONG;

const LEVEL_1_INDEX_MASK_LONG: usize = (1 << LEVEL_1_TABLE_SHIFT_LONG) - 1;
const LEVEL_2_INDEX_MASK_LONG: usize = (1 << LEVEL_2_TABLE_SHIFT_LONG) - 1;
const LEVEL_3_INDEX_MASK_LONG: usize = (1 << LEVEL_3_TABLE_SHIFT_LONG) - 1;

const TABLE_SIZE_LONG: usize = super::get_page_size();

/// If using 40-bit virtual addresses, bits [39:32] of the address are bits
/// [7:0] of the high descriptor word.
const ADDR_MASK_HIGH_MASK_LONG: usize = 0xff;

/// When using 4 KiB pages with a 32-bit output address, bits [31:12] are the
/// physical address of a table or page pointer. Bits [31:21] are the physical
/// address of a 2 MiB block at Level 2.
const TABLE_OR_PAGE_LOW_MASK_LONG: usize = 0xffff_f000;
const LEVEL_1_BLOCK_LOW_MASK_LONG: usize = 0xc000_0000;
const LEVEL_2_BLOCK_LOW_MASK_LONG: usize = 0xffe0_0000;

/// Bits [1:0] are the entry type. 0b11 indicates a table pointer entry at
/// Levels 1 and 2, and indicates a page entry at Level 3. 0b01 indicates a
/// block entry at Levels 1 and 2, but is invalid at Level 3. 0bn0 always
/// indicates an invalid entry regardless of `n`.
const MM_PAGE_TABLE_FLAG_LONG: usize = 0b11 << 0;
const MM_PAGE_FLAG_LONG: usize = 0b11 << 0;
const MM_BLOCK_FLAG_LONG: usize = 0b01 << 0;
const MM_ACCESS_FLAG_LONG: usize = 0b1 << 10;

/// The start code has already configured the MAIR registers. Only the memory
/// type indices are needed here. See `mm.s`.
const MM_NORMAL_MAIR_IDX_LONG: usize = 0x0;
const MM_DEVICE_MAIR_IDX_LONG: usize = 0x1;

const TYPE_MASK: usize = 0x3;

/// The maximum number of local mappings a task can maintain.
const MAX_LOCAL_MAPPINGS: usize = super::get_page_size() >> super::get_page_table_entry_shift();

/// Translation table level. LPAE supports up to 3 levels of translation.
#[derive(Copy, Clone, PartialEq)]
enum TableLevel {
  Level1,
  Level2,
  Level3,
}

/// Direct map a range of physical addresses to a virtual address space.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `split` - The virtual memory split.
/// * `pages_start` - The physical address of the task's starting page table.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
/// * `strategy` - The mapping strategy.
///
/// # Description
///
/// Direct mapping maps a physical address PA to a virtual address VA where
/// `VA = PA + virtual base`.
///
/// # Assumptions
///
/// * The physical address of the starting page table is in low memory.
/// * The allocator *must* allocate pages in low memory.
pub fn direct_map_memory(
  virtual_base: usize,
  pages_start: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) {
  let virt = virtual_base + base;

  fill_table(
    virtual_base,
    get_first_table_level(virtual_base, virt),
    pages_start,
    virt,
    base,
    size,
    device,
    allocator,
    strategy,
  );
}

/// Map a range of physical addresses to a virtual address space.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `pages_start` - The physical address of the task's starting page table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
/// * `strategy` - The mapping strategy.
///
/// # Description
///
/// Mapping a physical address PA to a virtual address VA where
/// `VA = (PA - base) + virt`.
///
/// # Assumptions
///
/// * The physical address of the starting page table is in low memory.
/// * The allocator *must* allocate pages in low memory.
pub fn map_memory(
  virtual_base: usize,
  pages_start: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) {
  fill_table(
    virtual_base,
    get_first_table_level(virtual_base, virt),
    pages_start,
    virt,
    base,
    size,
    device,
    allocator,
    strategy,
  );
}

/// Maps a thread-local table into the kernel's address space.
///
/// # Parameters
///
/// * `pages_start` - The physical address of the starting kernel page table.
/// * `local_virt` - The virtual address of the core's thread local area.
/// * `table_addr` - The physical address of the task's local mappings table.
///
/// # Description
///
/// If using a 2/2 split, the kernel has a Level 1 table, and we need to get the
/// address of the Level 2 table that covers the thread local area. Otherwise,
/// if using a 3/1 split, the kernel starts at a Level 2 table.
///
/// The thread-local mapping table is mapped by adding a table entry to the
/// Level 2 table.
///
/// # Assumptions
///
/// The Level 1 and Level 2 page tables are in low memory.
pub fn map_thread_local_table(pages_start: usize, local_virt: usize, table_addr: usize) {
  let virtual_base = super::get_kernel_virtual_base();
  let start_level = get_first_table_level(virtual_base, local_virt);
  let l2_addr: usize;

  if start_level == TableLevel::Level1 {
    let table = get_table(virtual_base + pages_start);
    let idx = get_descriptor_index(local_virt, start_level);
    l2_addr = get_phys_addr_from_descriptor(start_level, table[idx], table[idx + 1]).unwrap();
  } else {
    l2_addr = pages_start;
  }

  let l2_vaddr = virtual_base + l2_addr;
  let idx = get_descriptor_index(local_virt, TableLevel::Level2);
  let desc_vaddr = l2_vaddr + (idx << bits::WORD_SHIFT);
  let (desc, desc_high) = make_pointer_descriptor(TableLevel::Level2, table_addr).unwrap();

  unsafe {
    mmu_update_table_entry_local(desc_vaddr, local_virt, desc, desc_high);
  }
}

/// Map a page in the task's local mappings.
///
/// # Parameters
///
/// * `table` - The task's local mapping table.
/// * `section_vaddr` - The base virtual address of the core's local section.
/// * `page_addr` - The physical address of the page to map.
/// * `count` - The number of mappings currently in the table.
/// * `device` - Whether this page maps to device memory.
///
/// # Description
///
/// Adds the page to the task's local mappings and invalidates the current
/// core's TLB for the virtual address assigned to the page.
///
/// # Returns
///
/// The virtual address assigned to the page.
pub fn map_page_local(
  table: &mut [usize],
  section_vaddr: usize,
  page_addr: usize,
  count: usize,
  device: bool,
) -> usize {
  assert!(count < MAX_LOCAL_MAPPINGS);

  let idx = count << 1;
  let page_vaddr = section_vaddr + (count << super::get_page_shift());
  let desc_vaddr = ptr::addr_of!(table[idx]) as usize;
  let (desc, desc_high) = make_descriptor(TableLevel::Level3, page_addr, device).unwrap();

  unsafe {
    mmu_update_table_entry_local(desc_vaddr, page_vaddr, desc, desc_high);
  }

  page_vaddr
}

/// Unmaps the most recent page from the task's local mappings.
///
/// # Parameters
///
/// * `table` - The task's local mapping table.
/// * `section_vaddr` - The base virtual address of the core's local section.
/// * `count` - The number of mappings currently in the table.
///
/// # Description
///
/// Removes the most recently added page from the task's local mappings table
/// and invalidates the current core's TLB for the virtual address assigned to
/// the page.
pub fn unmap_page_local(table: &mut [usize], section_vaddr: usize, count: usize) {
  assert!(count > 0);

  let idx = (count - 1) << 1;

  // A null entry is a placeholder for a local mapping that is in low memory.
  if table[idx] == 0 {
    return;
  }

  let page_vaddr = section_vaddr + ((count - 1) << super::get_page_shift());
  let desc_vaddr = ptr::addr_of!(table[idx]) as usize;

  unsafe {
    mmu_update_table_entry_local(desc_vaddr, page_vaddr, 0, 0);
  }
}

/// Get the first table level to translate a given virtual address.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `virt_addr` - The virtual address.
///
/// # Description
///
///   NOTE: The MMU will automatically skip Level 1 translation if the size of
///         a segment is 1 GiB or less. In a 3/1 split, the MMU expects that
///         TTBR1 points to the kernel segment's Level 2 table.
//
/// # Returns
///
/// Level 2 if the virtual address is in the kernel address space and a 3/1
/// split is in use. Otherwise, Level 1.
fn get_first_table_level(virtual_base: usize, virt_addr: usize) -> TableLevel {
  let split = super::get_kernel_config().vm_split;

  if (virt_addr >= virtual_base) && (split == 3) {
    TableLevel::Level2
  } else {
    TableLevel::Level1
  }
}

/// Wrapper for strategy-specific fill functions.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `table_addr` - The physical address of the current page table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
/// * `strategy` - The mapping strategy.
fn fill_table(
  virtual_base: usize,
  table_level: TableLevel,
  table_addr: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) {
  match strategy {
    MappingStrategy::Compact => {
      fill_table_compact(virtual_base, table_level, table_addr, virt, base, size, device, allocator)
    }
    MappingStrategy::Granular => fill_table_granular(
      virtual_base,
      table_level,
      table_addr,
      virt,
      base,
      size,
      device,
      allocator,
    ),
  }
}

/// Fills a page table with entries for the specified range using sections to
/// reduce the number of entries required.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `table_addr` - The physical address of the current page table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
///
/// # Details
///
/// The "classic" ARM MMU supports two levels of address translation using
/// 32-bit page table descriptors.
///
///     Level 1       ->  Level 2
///     4096 Entries      256 Entries
///     Covers 4 GiB      Covers 1 MiB
///
/// With short page table descriptors, if the address space is split between
/// user space and kernel space, the user address space cannot be larger than
/// 2 GiB (even 2/2 split).
///
///   NOTE: Propeller *does not* support the "classic" system.
///
/// When an ARM CPU implements the Large Physical Address Extensions, it
/// supports the long page table descriptor format. Instead of the "classic"
/// two-level translation tables, the MMU supports three levels of address
/// translation using 64-bit page table descriptors.
///
///     Level 1       ->  Level 2       -> Level 3
///     4 Entries         512 Entries      512 Entries
///     Covers 4 GiB      Covers 1 GiB     Covers 2 MiB
///
/// Additionally, LPAE allows configuring the MMU to increase the size of the
/// user address space making a 3/1 split possible.
///
///   NOTE: The MMU will automatically skip Level 1 translation if the size of
///         a segment is 1 GiB or less. In a 3/1 split, the MMU expects that
///         TTBR1 points to the kernel segment's Level 2 table.
///
/// Section entries at Level 2 may be used to map a 2 MiB section and avoid
/// using a Level 3 table.
///
/// This function requires the base address and virtual address to be page-
/// aligned. If the virtual address is not also section-aligned, Level 3 page
/// entries will be used until it is section-aligned. Level 2 section entries
/// will be used thereafter to reduce the number of tables required.
fn fill_table_compact(
  virtual_base: usize,
  table_level: TableLevel,
  table_addr: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
) {
  let page_size = super::get_page_size();
  let section_size = super::get_section_size();

  assert!(bits::is_aligned(virt, page_size));
  assert!(bits::is_aligned(base, page_size));

  let entry_size = get_table_entry_size(table_level);
  let mut virt = virt;
  let mut base = base;
  let mut size = size;
  let table = get_table(virtual_base + table_addr);

  while size >= page_size {
    let idx = get_descriptor_index(virt, table_level);
    let aligned = bits::is_aligned(virt, section_size);
    let mut fill_size = entry_size;
    let desc: usize;
    let desc_high: usize;

    // If the base virtual address is not aligned on the entry size or the size
    // of the block is less than the entry size, a block entry cannot be used.
    if !aligned || size < entry_size {
      // If the base virtual address is not aligned, only map enough to align,
      // then use blocks. Otherwise, fill out the remaining size.
      if !aligned {
        fill_size = size & (entry_size - 1);
      } else {
        fill_size = size;
      }

      (desc, desc_high) = alloc_table_and_fill(
        virtual_base,
        table_level,
        table[idx],
        table[idx + 1],
        virt,
        base,
        fill_size,
        device,
        allocator,
        MappingStrategy::Compact,
      );
    } else {
      (desc, desc_high) = make_descriptor(table_level, base, device).unwrap();
    }

    table[idx] = desc;
    table[idx + 1] = desc_high;

    virt += fill_size;
    base += fill_size;
    size -= fill_size;
  }
}

/// Fills a page table with entries for the specified range using individual
/// page entries.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `table_addr` - The physical address of the current page table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
///
/// # Description
///
///   NOTE: This function requires the base address and virtual address to be
///         page-aligned.
fn fill_table_granular(
  virtual_base: usize,
  table_level: TableLevel,
  table_addr: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
) {
  let page_size = super::get_page_size();

  assert!(bits::is_aligned(virt, page_size));
  assert!(bits::is_aligned(base, page_size));

  let entry_size = get_table_entry_size(table_level);
  let mut virt = virt;
  let mut base = base;
  let mut size = size;
  let table = get_table(virtual_base + table_addr);

  loop {
    let idx = get_descriptor_index(virt, table_level);
    let desc: usize;
    let desc_high: usize;

    // For levels 1 and 2, allocate new tables as necessary and descend to the
    // next level down. At level 3, add individual page entries.
    if table_level != TableLevel::Level3 {
      (desc, desc_high) = alloc_table_and_fill(
        virtual_base,
        table_level,
        table[idx],
        table[idx + 1],
        virt,
        base,
        size,
        device,
        allocator,
        MappingStrategy::Granular,
      );
    } else {
      (desc, desc_high) = make_descriptor(table_level, base, device).unwrap();
    }

    table[idx] = desc;
    table[idx + 1] = desc_high;

    // If the size of the block is smaller than the entry size, there is nothing
    // left to do.
    if size <= entry_size {
      break;
    }

    virt += entry_size;
    base += entry_size;
    size -= entry_size;
  }
}

/// Given a table level, returns the size covered by a single entry.
///
/// # Parameters
///
/// * `table_level` - The table level of interest.
///
/// # Returns
///
/// The size covered by a single entry in bytes.
fn get_table_entry_size(table_level: TableLevel) -> usize {
  match table_level {
    TableLevel::Level1 => 1 << LEVEL_1_SHIFT_LONG,
    TableLevel::Level2 => 1 << LEVEL_2_SHIFT_LONG,
    TableLevel::Level3 => super::get_page_size(),
  }
}

/// Given a table level, return the next table level down in the translation
/// hierarchy assuming LPAE.
///
/// # Parameters
///
/// * `table_level` - The current table level.
///
/// # Returns
///
/// The next table level, or None for Level 3.
fn get_next_table(table_level: TableLevel) -> Option<TableLevel> {
  match table_level {
    TableLevel::Level1 => Some(TableLevel::Level2),
    TableLevel::Level2 => Some(TableLevel::Level3),
    TableLevel::Level3 => None,
  }
}

/// Get the physical address for either the next table from a descriptor.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `desc` - The lower 32-bits of the descriptor.
/// * `desc_high` - The upper 32-bits of the descriptor.
///
/// # Description
///
///   NOTE: Does not support LPAE 40-bit pointers. Bits [7:0] of `desc_high`
///         must be zero.
///
/// # Returns
///
/// The physical address, or None if the descriptor is invalid.
fn get_phys_addr_from_descriptor(
  table_level: TableLevel,
  desc: usize,
  desc_high: usize,
) -> Option<usize> {
  if desc_high & ADDR_MASK_HIGH_MASK_LONG != 0 {
    return None;
  }

  match desc & TYPE_MASK {
    MM_PAGE_TABLE_FLAG_LONG => Some(desc & TABLE_OR_PAGE_LOW_MASK_LONG),
    MM_BLOCK_FLAG_LONG => match table_level {
      TableLevel::Level1 => Some(desc & LEVEL_1_BLOCK_LOW_MASK_LONG),
      TableLevel::Level2 => Some(desc & LEVEL_2_BLOCK_LOW_MASK_LONG),
      _ => None,
    },
    _ => None,
  }
}

/// Create a table descriptor appropriate to the specified table level.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `phys_addr` - The physical address of the block or page.
/// * `device` - Whether this block or page maps to device memory.
///
/// # Description
///
/// The table level must be 2 or 3. The Level 1 table can only point to Level 2
/// tables.
///
/// # Returns
///
/// A tuple with the low and high 32-bits of the descriptor, or None if it is
/// not possible to make a descriptor.
fn make_descriptor(
  table_level: TableLevel,
  phys_addr: usize,
  device: bool,
) -> Option<(usize, usize)> {
  let mair_idx = if device {
    MM_DEVICE_MAIR_IDX_LONG
  } else {
    MM_NORMAL_MAIR_IDX_LONG
  };

  match table_level {
    TableLevel::Level1 => {
      Some(make_block_descriptor(phys_addr & LEVEL_1_BLOCK_LOW_MASK_LONG, mair_idx))
    }
    TableLevel::Level2 => {
      Some(make_block_descriptor(phys_addr & LEVEL_2_BLOCK_LOW_MASK_LONG, mair_idx))
    }
    TableLevel::Level3 => {
      Some(make_page_descriptor(phys_addr & TABLE_OR_PAGE_LOW_MASK_LONG, mair_idx))
    }
  }
}

/// Make a Level 1 or Level 2 block descriptor.
///
/// # Parameters
///
/// * `phys_addr` - The physical address of the block or page.
/// * `mair_idx` - The block attributes MAIR index.
///
/// # Description
///
/// This function should not be called directly.
///
/// # Returns
///
/// A tuple with the low and high 32-bits of the descriptor.
fn make_block_descriptor(phys_addr: usize, mair_idx: usize) -> (usize, usize) {
  (phys_addr | (mair_idx << 2) | MM_ACCESS_FLAG_LONG | MM_BLOCK_FLAG_LONG, 0)
}

/// Make a Level 3 page descriptor.
///
/// # Parameters
///
/// * `phys_addr` - The physical address of the block or page.
/// * `mair_idx` - The page attributes MAIR index.
///
/// # Description
///
/// This function should not be called directly.
///
/// # Returns
///
/// A tuple with the low and high 32-bits of the descriptor.
fn make_page_descriptor(phys_addr: usize, mair_idx: usize) -> (usize, usize) {
  (phys_addr | (mair_idx << 2) | MM_ACCESS_FLAG_LONG | MM_PAGE_FLAG_LONG, 0)
}

/// Determine if a descriptor is a table pointer.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `desc` - The lower 32-bits of the descriptor.
/// * `desc_high` - The upper 32-bits of the descriptor.
///
/// # Returns
///
/// True if the descriptor is a page table pointer, false otherwise.
fn is_pointer_entry(table_level: TableLevel, desc: usize, _desc_high: usize) -> bool {
  match table_level {
    TableLevel::Level3 => false,
    _ => desc & TYPE_MASK == MM_PAGE_TABLE_FLAG_LONG,
  }
}

/// Make a pointer descriptor to a lower level page table.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `phys_addr` - The physical address of the table.
///
/// # Returns
///
/// A tuple with the low and high 32-bits of the descriptor, or None if the
/// table level is invalid.
fn make_pointer_descriptor(table_level: TableLevel, phys_addr: usize) -> Option<(usize, usize)> {
  match table_level {
    TableLevel::Level3 => None,
    _ => {
      if !bits::is_aligned(phys_addr, super::get_page_size()) {
        return None;
      }

      // Bits [11:2] are ignored at Levels 1 and 2. However, we want to be able
      // to read/write the table through the recursive map. So, fill in the MAIR
      // index in bits [4:2] and the access flag in bit 10. Leaving bits [7:6]
      // as zero makes the page read/write for the kernel.
      Some((
        phys_addr | (MM_NORMAL_MAIR_IDX_LONG << 2) | MM_ACCESS_FLAG_LONG | MM_PAGE_TABLE_FLAG_LONG,
        0,
      ))
    }
  }
}

/// Get the descriptor index for a virtual address in the specified table.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address.
/// * `table_level` - The table level for the index.
///
/// # Description
///
///     +----+--------+--------+-----------+
///     | L1 |   L2   |   L3   |  Offset   |
///     +----+--------+--------+-----------+
///     31  30       21       12           0
///
///   NOTE: The index is in 32-bit words. When using LPAE, the index returned
///         by this function, `N`, is the low 32-bits of the descriptor while
///         the index `N + 1` is the high 32-bits.
///
/// # Returns
///
/// The index into the table at the specified level.
fn get_descriptor_index(virt_addr: usize, table_level: TableLevel) -> usize {
  match table_level {
    TableLevel::Level1 => ((virt_addr >> LEVEL_1_SHIFT_LONG) & LEVEL_1_INDEX_MASK_LONG) << 1,
    TableLevel::Level2 => ((virt_addr >> LEVEL_2_SHIFT_LONG) & LEVEL_2_INDEX_MASK_LONG) << 1,
    TableLevel::Level3 => ((virt_addr >> LEVEL_3_SHIFT_LONG) & LEVEL_3_INDEX_MASK_LONG) << 1,
  }
}

/// Get a memory slice for the table at a given address.
///
/// # Parameters
///
/// * `table_vaddr` - The table virtual address.
///
/// # Assumptions
///
/// Assumes all tables to be TABLE_SIZE_LONG including Level 1 tables.
///
/// # Returns
///
/// A slice of the correct size for the table level.
fn get_table(table_vaddr: usize) -> &'static mut [usize] {
  unsafe {
    // Note the shift right by 2 instead of 3. The slice is 32 bits, not 64.
    slice::from_raw_parts_mut(table_vaddr as *mut usize, TABLE_SIZE_LONG >> 2)
  }
}

/// Allocates a new page table if necessary, then fills the table with entries
/// for the specified range of memory.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `desc` - The current descriptor in the table.
/// * `desc_high` - High 32-bits of a long descriptor (0 if LPAE not supported).
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
/// * `strategy` - The mapping strategy.
///
/// # Description
///
/// The current table must be Level 1 or 2. Level 3 tables can only point to
/// pages.
///
/// Recursion is bounded by the table levels.
///
/// # Returns
///
/// A tuple with the low and high 32-bits of the descriptor.
fn alloc_table_and_fill(
  virtual_base: usize,
  table_level: TableLevel,
  desc: usize,
  desc_high: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) -> (usize, usize) {
  let next_level = get_next_table(table_level).unwrap();
  let mut next_addr = get_phys_addr_from_descriptor(table_level, desc, desc_high).unwrap();
  let mut desc = desc;
  let mut desc_high = desc_high;

  // TODO: It is probably fine to overwrite a section descriptor. If the memory
  //       configuration is overwriting itself, then we probably have something
  //       wrong and an exception is the right outcome if the configuration is
  //       invalid.
  if !is_pointer_entry(table_level, desc, desc_high) {
    // Let an assert occur if we cannot allocate a table.
    next_addr = allocator.alloc_table().unwrap();

    unsafe {
      // Zero out the table. Any entry in the table with bits 0 and 1 set to 0
      // is invalid.
      ptr::write_bytes((virtual_base + next_addr) as *mut u8, 0, TABLE_SIZE_LONG);
    }

    (desc, desc_high) = make_pointer_descriptor(table_level, next_addr).unwrap();
  }

  fill_table(virtual_base, next_level, next_addr, virt, base, size, device, allocator, strategy);

  (desc, desc_high)
}
