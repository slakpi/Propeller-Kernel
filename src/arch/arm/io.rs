//! ARM I/O Memory Management
//!
//!   TODO: Consider moving most of this up to arch::io and have it call down
//!         here to get available block information. That way, all architectures
//!         share the I/O address map for tracking available / in-use blocks.

use crate::debug_print;
use crate::mm;
use crate::mm::page_allocator::BuddyPageAllocator;
use crate::support::bits;
use crate::support::memory::{MappingStrategy, MemoryZone, PageAllocator};
use crate::support::rb_tree::RedBlackTree;
use crate::sync::{SpinLock, SpinLockGuard};
use core::ptr;

/// Tracks available and in-use virtual address blocks for I/O mapping.
struct IoAddressMap<'a> {
  avail: RedBlackTree<'a, BuddyPageAllocator<'a>, usize, usize>,
  in_use: RedBlackTree<'a, BuddyPageAllocator<'a>, usize, usize>,
}

impl<'a> IoAddressMap<'a> {
  /// Construct a new I/O virtual address map.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator for the tracking data structures.
  /// * `base` - The virtual base address of the area being tracked.
  /// * `pages` - The number of pages in the virtual address area being tracked.
  fn new(allocator: &'a SpinLock<BuddyPageAllocator<'a>>, base: usize, pages: usize) -> Self {
    let mut map = Self {
      avail: RedBlackTree::new(allocator),
      in_use: RedBlackTree::new(allocator),
    };

    _ = map.avail.insert(pages, base);

    map
  }

  /// Get a virtual address block from the area being tracked.
  ///
  /// # Parameters
  ///
  /// * `pages` - The requested size of the block in pages.
  ///
  /// # Returns
  ///
  /// The base virtual address of a block if a block of the requested size is
  /// available. None, if a block of the requested size is not available or the
  /// size is 0.
  fn get_virt_addr_block(&mut self, pages: usize) -> Option<usize> {
    if pages == 0 {
      return None;
    }

    let page_shift = super::init::get_page_shift();
    let size = pages << page_shift;

    // Check for a sufficiently large block and remove it if we find one.
    let block_index = self.avail.lower_bound(pages)?;
    let (block_pages, block_base) = self.avail.remove(block_index)?;
    let base = block_base;

    // If the block will have at least a page remaining after slicing off the
    // pages being mapped, update the block and put it back into the tree.
    // Otherwise, drop it.
    if block_pages > pages {
      _ = self.avail.insert(block_pages - pages, block_base + size);
    }

    // Mark the block as in-use.
    self.in_use.insert(base, pages);

    Some(base)
  }

  /// Release a virtual address block back to the map.
  ///
  /// # Parameters
  ///
  /// * `virt_addr` - The base virtual address of the block to release.
  ///
  /// # Description
  ///
  /// The virtual address must be an address returned from
  /// `get_virt_addr_block()`.
  ///
  /// # Returns
  ///
  /// The number of pages in the block, or None if the virtual address was not
  /// issued by `get_virt_addr_block()` or has already been released.
  fn release_virt_addr_block(&mut self, virt_addr: usize) -> Option<usize> {
    let (_, pages) = self.in_use.erase(virt_addr)?;
    let page_shift = super::init::get_page_shift();
    let size = pages << page_shift;

    for (index, kv) in self.avail.iter().enumerate() {
      let new_base: usize;

      // The block should not be in both trees.
      assert_ne!(virt_addr, *kv.1);

      // Check to see if this block can be merged back with a block in already
      // in the tree. If the higher base address minus the lower base address is
      // the size of the block starting at the lower base address, merge the two
      // blocks.
      if virt_addr < *kv.1 {
        let temp = *kv.1 - virt_addr;

        if temp > size {
          continue;
        }

        // If temp is not greater than size, they have to be equal. Otherwise,
        // the two blocks overlap and that should never happen.
        assert_eq!(temp, size);

        new_base = virt_addr;
      } else {
        let temp = virt_addr - *kv.1;

        // No reason to continue looking.
        if temp > size {
          break;
        }

        // If temp is not greater than kv.0 (in bytes), they have to be equal.
        // Otherwise, the two blocks overlap and that should never happen.
        assert_eq!(temp, *kv.0 << page_shift);

        new_base = *kv.1;
      }

      let new_pages = pages + *kv.0;
      self.avail.remove(index);
      _ = self.avail.insert(new_pages, new_base);

      // The iterator is no longer valid after performing the remove, but we are
      // done anyway.
      break;
    }

    Some(pages)
  }
}

/// TODO: Is this the right place for the lock? Will the pages change anywhere
///       else in the kernel after initialization?
static mut KERNEL_PAGE_LOCK: SpinLock<()> = SpinLock::new(());

/// I/O address map.
static mut IO_ADDR_MAP: Option<SpinLock<IoAddressMap<'static>>> = None;

/// Initialize the I/O memory management module.
///
/// # Description
///
///   NOTE: Must only be called from the main io module during initialization.
pub fn init() {
  let base = super::init::get_driver_area_virtual_base();
  let size = super::init::get_driver_area_size();
  let pages = size >> super::init::get_page_shift();

  debug_print!(" Driver area: {:#x} - {:#x}\n", base, base + size - 1);

  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();

  unsafe {
    IO_ADDR_MAP = Some(SpinLock::new(IoAddressMap::new(alloc, base, pages)));
  }
}

/// Map a block of pages as I/O memory.
///
/// # Parameters
///
/// * `phys_addr` - The base physical address of the block.
/// * `pages` - The number of pages to map.
///
/// # Returns
///
/// The virtual address of the mapped block, or None if no virtual address
/// blocks of the required size are available.
pub fn map_io(phys_addr: usize, pages: usize) -> Option<usize> {
  // We can only map full, aligned pages.
  if !bits::is_aligned(phys_addr, super::init::get_page_size()) || pages == 0 {
    return None;
  }

  // Allocate a virtual address block for the mapping.
  let virt_addr = get_io_addr_map().lock().get_virt_addr_block(pages)?;

  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();

  // Lock the kernel page tables and map the memory.
  let table_guard = lock_kernel_page_tables();
  super::mm::map_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_vm_split(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    phys_addr,
    pages << super::init::get_page_shift(),
    true,
    alloc.lock().as_mut(),
    MappingStrategy::Granular,
  );
  drop(table_guard);

  super::mm::invalidate_tlb_by_va(virt_addr);

  Some(virt_addr)
}

/// Unmap a block of I/O memory.
///
/// # Parameters
///
/// * `virt_addr` - The base virtual address of the block.
///
/// # Description
///
/// The virtual address must be an address returned by `map_io()`.
pub fn unmap_io(virt_addr: usize) {
  // Attempt to release the virtual address block.
  let Some(pages) = get_io_addr_map().lock().release_virt_addr_block(virt_addr) else {
    return;
  };

  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();

  // Lock the kernel page tables and map the memory.
  let table_guard = lock_kernel_page_tables();
  super::mm::unmap_memory(
    super::init::get_kernel_virtual_base(),
    super::init::get_vm_split(),
    super::init::get_kernel_pages_start(),
    virt_addr,
    pages << super::init::get_page_shift(),
    alloc.lock().as_mut(),
  );
  drop(table_guard);

  super::mm::invalidate_tlb_by_va(virt_addr);
}

/// Get the I/O address map.
fn get_io_addr_map() -> &'static SpinLock<IoAddressMap<'static>> {
  let map = unsafe { ptr::addr_of!(IO_ADDR_MAP).as_ref().unwrap() };
  map.as_ref().unwrap()
}

/// Lock the kernel page tables.
fn lock_kernel_page_tables() -> SpinLockGuard<'static, ()> {
  unsafe { ptr::addr_of!(KERNEL_PAGE_LOCK).as_ref().unwrap() }.lock()
}
