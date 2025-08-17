//! AArch64 Memory Management

use crate::mm::{MappingStrategy, table_allocator::TableAllocator};
use crate::support::bits;
use core::{cmp, ptr, slice};

/// All levels use nine bits of the address for table indices.
const TABLE_SHIFT: usize = 9;
const INDEX_MASK: usize = (1 << TABLE_SHIFT) - 1;

const LEVEL_4_SHIFT: usize = super::get_page_shift();
const LEVEL_3_SHIFT: usize = LEVEL_4_SHIFT + TABLE_SHIFT;
const LEVEL_2_SHIFT: usize = LEVEL_3_SHIFT + TABLE_SHIFT;
const LEVEL_1_SHIFT: usize = LEVEL_2_SHIFT + TABLE_SHIFT;

/// Tables are a single page at all levels.
const TABLE_SIZE: usize = super::get_page_size();

/// Mask off bits [63:48] of the descriptor containing the upper attributes.
const LOW_DESCRIPTOR_MASK: usize = usize::MAX & ((1 << 48) - 1);

/// Bits [47:n] of the descriptor are the physical address where `n` is 39, 30,
/// 21, or 12 for Levels 1, 2, 3, and 4 respectively.
const LEVEL_4_ADDR_MASK: usize = LOW_DESCRIPTOR_MASK & !((1 << LEVEL_4_SHIFT) - 1);
const LEVEL_3_ADDR_MASK: usize = LOW_DESCRIPTOR_MASK & (LEVEL_4_ADDR_MASK << TABLE_SHIFT);
const LEVEL_2_ADDR_MASK: usize = LOW_DESCRIPTOR_MASK & (LEVEL_3_ADDR_MASK << TABLE_SHIFT);
const LEVEL_1_ADDR_MASK: usize = LOW_DESCRIPTOR_MASK & (LEVEL_2_ADDR_MASK << TABLE_SHIFT);

const MM_PAGE_TABLE_FLAG: usize = 0x3 << 0;
const MM_PAGE_FLAG: usize = 0x3 << 0;
const MM_BLOCK_FLAG: usize = 0x1 << 0;
const _MM_RO_FLAG: usize = 0x10 << 6;
const MM_ACCESS_FLAG: usize = 0x1 << 10;

/// The start code has already configured the MAIR registers. Only the memory
/// type indices are needed here. See `mm.s`.
const MM_NORMAL_MAIR_IDX: usize = 0x0;
const MM_DEVICE_MAIR_IDX: usize = 0x1;

const TYPE_MASK: usize = 0x3;

/// Translation table level.
#[derive(Clone, Copy, PartialEq)]
enum TableLevel {
  Level1,
  Level2,
  Level3,
  Level4,
}

/// Direct map a range of physical addresses to a virtual address space.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `pages_start` - The address of the kernel's starting page table.
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
pub fn direct_map_memory(
  virtual_base: usize,
  pages_start: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) {
  fill_table(
    virtual_base,
    TableLevel::Level1,
    pages_start,
    virtual_base + base,
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
/// * `pages_start` - The address of the task's starting page table.
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
    TableLevel::Level1,
    pages_start,
    virt,
    base,
    size,
    device,
    allocator,
    strategy,
  );
}

/// Wrapper for strategy-specific fill functions.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `table_addr` - The address of the current page table.
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
/// compact the tables.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `table_addr` - The address of the current page table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
///
/// # Details
///
/// AArch64 provides four levels of address space translation. With 4 KiB pages,
/// the page tables can address 256 TiB of memory:
///
///     Level 1   ->    Level 2   ->    Level 3   ->    Level 4
///     Covers          Covers          Covers          Covers
///     256 TiB         512 GiB         1 GiB           2 MiB
///
/// Section entries at Level 2 and 3 may be used to map larger blocks of memory
/// and avoid lower level translation. A section entry at Level 2 can map a 1
/// GiB block and avoid Level 3 and 4 translation. A section entry at Level 3
/// can map a 2 MiB block and avoid Level 4 translation.
///
/// This function requires the base address and virtual address to be page-
/// aligned. If the virtual address is not also section-aligned, lower level
/// tables are used until it is aligned and section entries are used thereafter
/// at that level.
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

  assert!(bits::is_aligned(base, page_size));
  assert!(bits::is_aligned(virt, page_size));

  let entry_size = get_table_entry_size(table_level);
  let mut virt = virt;
  let mut base = base;
  let mut size = size;
  let table = get_table(virtual_base + table_addr);

  while size >= page_size {
    let idx = get_descriptor_index(virt, table_level);
    let aligned = bits::is_aligned(virt, entry_size);
    let mut fill_size = entry_size;

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

      table[idx] = alloc_table_and_fill(
        virtual_base,
        table_level,
        table[idx],
        virt,
        base,
        fill_size,
        device,
        allocator,
        MappingStrategy::Compact,
      );
    } else {
      table[idx] = make_descriptor(table_level, base, device).unwrap();
    }

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
/// * `table_addr` - The address of the current page table.
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

    // For levels 1, 2, and 3, allocate new tables as necessary and descend to
    // the next level down. At level 4, add individual page entries.
    if table_level != TableLevel::Level4 {
      table[idx] = alloc_table_and_fill(
        virtual_base,
        table_level,
        table[idx],
        virt,
        base,
        size,
        device,
        allocator,
        MappingStrategy::Granular,
      );
    } else {
      table[idx] = make_descriptor(table_level, base, device).unwrap();
    }

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
    TableLevel::Level1 => 1 << LEVEL_1_SHIFT,
    TableLevel::Level2 => 1 << LEVEL_2_SHIFT,
    TableLevel::Level3 => 1 << LEVEL_3_SHIFT,
    TableLevel::Level4 => 1 << LEVEL_4_SHIFT,
  }
}

/// Given a table level, return the next table level down in the translation
/// hierarchy.
///
/// # Parameters
///
/// * `table_level` - The current table level.
///
/// # Returns
///
/// The next table level, or None if Level 4 is specified.
fn get_next_table(table_level: TableLevel) -> Option<TableLevel> {
  match table_level {
    TableLevel::Level1 => Some(TableLevel::Level2),
    TableLevel::Level2 => Some(TableLevel::Level3),
    TableLevel::Level3 => Some(TableLevel::Level4),
    TableLevel::Level4 => None,
  }
}

/// Get the physical address for either the next table or memory block from a
/// descriptor.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `desc` - The descriptor.
///
/// # Returns
///
/// The physical address.
fn get_phys_addr_from_descriptor(table_level: TableLevel, desc: usize) -> usize {
  match table_level {
    TableLevel::Level1 => desc & LEVEL_1_ADDR_MASK,
    TableLevel::Level2 => desc & LEVEL_2_ADDR_MASK,
    TableLevel::Level3 => desc & LEVEL_3_ADDR_MASK,
    TableLevel::Level4 => desc & LEVEL_4_ADDR_MASK,
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
/// The table level must be 2, 3, or 4. The Level 1 table can only point to
/// Level 2 tables.
///
/// # Returns
///
/// The new descriptor.
fn make_descriptor(table_level: TableLevel, phys_addr: usize, device: bool) -> Option<usize> {
  let mair_idx = if device {
    MM_DEVICE_MAIR_IDX
  } else {
    MM_NORMAL_MAIR_IDX
  };

  let phys_addr = match table_level {
    TableLevel::Level1 => phys_addr & LEVEL_1_ADDR_MASK,
    TableLevel::Level2 => phys_addr & LEVEL_2_ADDR_MASK,
    TableLevel::Level3 => phys_addr & LEVEL_3_ADDR_MASK,
    TableLevel::Level4 => phys_addr & LEVEL_4_ADDR_MASK,
  };

  match table_level {
    TableLevel::Level2 | TableLevel::Level3 => Some(make_block_descriptor(phys_addr, mair_idx)),
    TableLevel::Level4 => Some(make_page_descriptor(phys_addr, mair_idx)),
    _ => None,
  }
}

/// Make a Level 2 or 3 block descriptor.
///
/// # Parameters
///
/// * `phys_addr` - The physical address of the block.
/// * `mair_idx` - The block attributes MAIR index.
///
/// # Description
///
/// This function should not be called directly.
///
/// # Returns
///
/// The new block descriptor.
fn make_block_descriptor(phys_addr: usize, mair_idx: usize) -> usize {
  phys_addr | (mair_idx << 2) | MM_ACCESS_FLAG | MM_BLOCK_FLAG
}

/// Make a Level 4 page descriptor.
///
/// # Parameters
///
/// * `phys_addr` - The physical address of the page.
/// * `mair_idx` - The page attributes MAIR index.
///
/// # Description
///
/// This function should not be called directly.
///
/// # Returns
///
/// The new page descriptor.
fn make_page_descriptor(phys_addr: usize, mair_idx: usize) -> usize {
  phys_addr | (mair_idx << 2) | MM_ACCESS_FLAG | MM_PAGE_FLAG
}

/// Determine if a descriptor is a table pointer.
///
/// # Parameters
///
/// * `table_level` - The table level of the new entry.
/// * `desc` - The descriptor.
///
/// # Returns
///
/// True if the descriptor is a page table pointer, false otherwise.
fn is_pointer_entry(table_level: TableLevel, desc: usize) -> bool {
  match table_level {
    TableLevel::Level1 | TableLevel::Level2 | TableLevel::Level3 => {
      desc & TYPE_MASK == MM_PAGE_TABLE_FLAG
    }
    _ => false,
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
/// The new pointer descriptor, or None if the table level is invalid.
fn make_pointer_entry(table_level: TableLevel, phys_addr: usize) -> Option<usize> {
  let phys_addr = match table_level {
    TableLevel::Level1 => phys_addr & LEVEL_1_ADDR_MASK,
    TableLevel::Level2 => phys_addr & LEVEL_2_ADDR_MASK,
    TableLevel::Level3 => phys_addr & LEVEL_3_ADDR_MASK,
    _ => return None,
  };

  Some(phys_addr | MM_PAGE_TABLE_FLAG)
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
/// With 4 KiB pages, the table indices are 9 bits each starting with Level 4 at
/// bit 12.
///
///     +---------+----+----+----+----+--------+
///     | / / / / | L1 | L2 | L3 | L4 | Offset |
///     +---------+----+----+----+----+--------+
///     63       48   39   30   21   12        0
///
/// # Returns
///
/// The index into the table at the specified level.
fn get_descriptor_index(virt_addr: usize, table_level: TableLevel) -> usize {
  match table_level {
    TableLevel::Level1 => (virt_addr >> LEVEL_1_SHIFT) & INDEX_MASK,
    TableLevel::Level2 => (virt_addr >> LEVEL_2_SHIFT) & INDEX_MASK,
    TableLevel::Level3 => (virt_addr >> LEVEL_3_SHIFT) & INDEX_MASK,
    TableLevel::Level4 => (virt_addr >> LEVEL_4_SHIFT) & INDEX_MASK,
  }
}

/// Get a memory slice for the table at a given address.
///
/// # Parameters
///
/// * `table_vaddr` - The table virtual address.
///
/// # Returns
///
/// A slice of the correct size for the table level.
fn get_table(table_vaddr: usize) -> &'static mut [usize] {
  unsafe { slice::from_raw_parts_mut(table_vaddr as *mut usize, TABLE_SIZE >> 3) }
}

/// Allocates a new page table if necessary, then fills the table with entries
/// for the specified range of memory.
///
/// # Parameters
///
/// * `virtual_base` - The kernel segment base address.
/// * `table_level` - The current table level.
/// * `desc` - The current descriptor in the table.
/// * `virt` - Base of the virtual address range.
/// * `base` - Base of the physical address range.
/// * `size` - Size of the physical address range.
/// * `device` - Whether this block or page maps to device memory.
/// * `allocator` - The allocator that will provide new table pages.
/// * `strategy` - The mapping strategy.
///
/// # Description
///
/// The current table must be Level 1, 2, or 3. Level 4 tables can only point to
/// pages.
///
/// # Returns
///
/// The new descriptor.
fn alloc_table_and_fill(
  virtual_base: usize,
  table_level: TableLevel,
  desc: usize,
  virt: usize,
  base: usize,
  size: usize,
  device: bool,
  allocator: &mut impl TableAllocator,
  strategy: MappingStrategy,
) -> usize {
  let next_level = get_next_table(table_level).unwrap();
  let mut next_addr = get_phys_addr_from_descriptor(table_level, desc);
  let mut desc = desc;

  // TODO: It is probably fine to overwrite a section descriptor. If the memory
  //       configuration is overwriting itself, then we probably have something
  //       wrong and an exception is the right outcome if the configuration is
  //       invalid.
  if !is_pointer_entry(table_level, desc) {
    next_addr = allocator.alloc_table().unwrap();

    unsafe {
      // Zero out the table. Any entry in the table with 0 in bit 0 is invalid.
      ptr::write_bytes((virtual_base + next_addr) as *mut u8, 0, TABLE_SIZE);
    }

    desc = make_pointer_entry(table_level, next_addr).unwrap();
  }

  fill_table(virtual_base, next_level, next_addr, virt, base, size, device, allocator, strategy);

  desc
}
