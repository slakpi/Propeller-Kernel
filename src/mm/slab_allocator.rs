//! Slab Allocator
//!
//! The slab allocator is based on Jeff Bonwick's '94 and '01 papers describing
//! a caching allocator for small kernel objects and breaks down as follows:
//!
//!      Core Cache Layer
//!     ------------------
//!        Bundle Layer
//!     -----------------
//!         Slab Layer
//!
//! The slab layer operates in a manner similar to BONWICK94. Given a type T,
//! the slab layer maintains linked lists of page blocks divided into individual
//! T objects. For example, assume T is 32 bytes and 24 bytes are needed for
//! slab metadata, a slab could be a single 4 KiB page divided into 127 T
//! objects and the metadata. The slab layer allocates blocks directly from a
//! provided page block allocator.
//!
//! Per BONWICK94, an object of type T should be no more than 1/8 the size of a
//! slab. So, the minimum size (S) of a slab is:
//!
//!     S = ( 7 * size of ObjectWrapper<T> ) + size of metadata
//!
//! The minimum number of pages (B) to allocate per slab is then:
//!
//!             --                                          --
//!     B = 2 ^ | log  [ ( S + page size - 1 ) / page size ] |
//!             |    2                                       |
//!
//! The page allocator requires `B <= 2^10`, thus the maximum object size the
//! slab allocator can accommodate is 512 KiB.
//!
//! The bundle layer operates in a manner similar to "magazines" in BONWICK01,
//! just with less automatic weapon analogies. The bundle layer does not auto-
//! tune bundle size currently, and uses a maximum object count of `M = 16`.
//!
//!   TODO: Implement bundle size auto-tuning.
//!
//! Bundles are allocated from a separate set of slabs.
//!
//! The core cache layer maintains a current and previous bundle for each core
//! in the system to implement the alloc and free algorithms from BONWICK01.
//!
//!   NOTE: The Slab Allocator is intended for allocating kernel objects. The
//!         allocator used to allocate slabs MUST allocate from linear memory.
//!
//! * https://www.usenix.org/legacy/publications/library/proceedings/usenix01/full_papers/bonwick/bonwick.pdf
//! * https://www.usenix.org/legacy/publications/library/proceedings/bos94/bonwick.html

#[cfg(feature = "module_tests")]
mod tests;

use crate::arch::memory::PageAllocator;
use crate::arch::{self, cpu};
use crate::support::bits;
use crate::sync::SpinLock;
#[cfg(feature = "module_tests")]
use crate::test;
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::{self, MaybeUninit};
use core::ops::{Deref, DerefMut};
use core::ptr;

/// Slab node metadata.
///
///     +-----------------+ Page 0
///     |                 |
///     +-               -+ Page 1
///     |                 |
///     +-   Objects     -+ ...
///     |                 |
///     +-               -+ Page N
///     |                 |
///     |.................|
///     | Slab Node       |
///     +-----------------+
///
/// The slab node metadata maintains a count of available objects in the slab, a
/// free list of objects, and its position in a doubly-linked list of slabs. All
/// pointers are linear virtual addresses.
///
/// Slab nodes use an XOR check as a sanity check.
#[repr(C)]
struct SlabNode {
  avail: usize,
  free: usize,
  prev: usize,
  next: usize,
  checksum: usize,
}

impl SlabNode {
  /// Construct a new slab node with a checksum.
  ///
  /// # Parameters
  ///
  /// * `avail` - The number of available objects.
  /// * `free` - The head of the free object list.
  /// * `next` - The next slab node.
  /// * `prev` - The previous slab node.
  const fn new(avail: usize, free: usize, next: usize, prev: usize) -> Self {
    Self {
      avail,
      free,
      prev,
      next,
      checksum: bits::xor_checksum(&[avail, free, prev, next]),
    }
  }

  /// Update a node's checksum with the current contents.
  fn update_checksum(&mut self) {
    self.checksum = bits::xor_checksum(&[self.avail, self.free, self.prev, self.next]);
  }

  /// Verify a node's checksum.
  ///
  /// # Returns
  ///
  /// True if the checksum is valid, false otherwise.
  fn verify_checksum(&self) -> bool {
    bits::xor_checksum(&[self.avail, self.free, self.prev, self.next]) == self.checksum
  }
}

/// An object wrapper encapsulates an object and a pointer to the next wrapper
/// in a free list. The pointer is a linear virtual address.
#[repr(C)]
struct ObjectWrapper<T> {
  obj: T,
  next: usize,
}

/// The Slab Manager implements the slab layer.
///
/// A slab manager maintains three lists of slabs: unused, in-use, and empty.
/// Unused slabs have no objects in-use. An in-use slab has some objects in use.
/// And, an empty slab has all objects in use. All pointers are linear virtual
/// addresses.
///
/// When allocating an object, it will be taken from the first in-use slab. If
/// no slabs are in use, the first unused slab will be moved to the in-use list
/// and an object will be allocated from it. If no unused slabs are available, a
/// new slab will be allocated, placed on the in-use list, and an object will be
/// allocated from it.
///
/// When freeing an object, it will be placed back in the free list of its
/// parent slab. If the parent slab was empty, it will be moved to the in-use
/// list. If all objects are now free on the parent slab, it will be moved to
/// the unused list.
///
///   NOTE: The slab manager is NOT thread-safe.
struct SlabManager<A, T> {
  slab_pages: usize,
  objs_per_slab: usize,
  unused: usize,
  in_use: usize,
  empty: usize,
  slabs_allocated: usize,
  objects_allocated: usize,
  objects_available: usize,
  _a: PhantomData<A>,
  _t: PhantomData<T>,
}

impl<'alloc, A, T> SlabManager<A, T>
where
  A: PageAllocator,
  T: Sized,
{
  /// Slabs should have space for at least seven objects to make the allocator
  /// efficient per BONWICK01.
  const MIN_OBJECTS_PER_SLAB: usize = 7;

  /// Construct a new slab manager.
  pub const fn new() -> Self {
    // Calculate the minimum number of pages assuming the minimum object count,
    // the size of a slab node, and rounding the page count up to the nearest
    // power of 2. Slabs must have a power-of-2 number of pages. This allows
    // determining the slab to which an object belongs simply by aligning the
    // object's address down by the slab size.
    let page_size = arch::get_page_size();
    let obj_size = size_of::<ObjectWrapper<T>>();
    let min_slab_size = obj_size * Self::MIN_OBJECTS_PER_SLAB + size_of::<SlabNode>();
    let slab_pages = 1 << bits::ceil_log2((min_slab_size + page_size - 1) / page_size);
    assert!(slab_pages <= A::MAX_BLOCK_PAGES);

    // Calculate the actual number of objects we can store in the final page
    // count.
    let slab_size = slab_pages * arch::get_page_size();
    let objs_per_slab = (slab_size - size_of::<SlabNode>()) / size_of::<ObjectWrapper<T>>();
    assert!(objs_per_slab > 0);

    SlabManager {
      slab_pages,
      objs_per_slab,
      unused: 0,
      in_use: 0,
      empty: 0,
      slabs_allocated: 0,
      objects_allocated: 0,
      objects_available: 0,
      _a: PhantomData,
      _t: PhantomData,
    }
  }

  /// Get a slab node from a node address.
  fn get_slab_node_mut(node_addr: usize) -> &'alloc mut SlabNode {
    let node = Self::get_slab_node_unchecked_mut(node_addr);
    assert!(node.verify_checksum());
    node
  }

  /// Get a slab node from a node address without verifying the checksum.
  fn get_slab_node_unchecked_mut(node_addr: usize) -> &'alloc mut SlabNode {
    unsafe { (node_addr as *mut SlabNode).as_mut().unwrap() }
  }

  /// Get an object wrapper from a wrapper address.
  fn get_object_mut(obj_addr: usize) -> &'alloc mut ObjectWrapper<T> {
    unsafe { (obj_addr as *mut ObjectWrapper<T>).as_mut().unwrap() }
  }

  /// Add a slab to a list and update the list head.
  ///
  /// # Parameters
  ///
  /// * `list` - The list head to update.
  /// * `node_addr` - The address of the node to add.
  fn add_slab_to_tail(list: &mut usize, node_addr: usize) {
    if node_addr == 0 {
      return;
    }

    let node = Self::get_slab_node_mut(node_addr);

    // If the list is empty, just point the node to itself and make it the head
    // of the list.
    if *list == 0 {
      *list = node_addr;
      node.prev = node_addr;
      node.next = node_addr;
      node.update_checksum();
      return;
    }

    let head = Self::get_slab_node_mut(*list);
    let prev = Self::get_slab_node_mut(head.prev);

    node.prev = head.prev;
    node.next = *list;
    node.update_checksum();

    head.prev = node_addr;
    head.update_checksum();

    prev.next = node_addr;
    prev.update_checksum();
  }

  /// Remove a slab from a list and update the list head.
  ///
  /// # Parameters
  ///
  /// * `list` - The list head to update.
  /// * `node_addr` The address of the node to remove.
  fn remove_slab_from_list(list: &mut usize, node_addr: usize) {
    if node_addr == 0 {
      return;
    }

    let node = Self::get_slab_node_mut(node_addr);

    // If the node points to itself, it is the only node in the list, just set
    // the head to zero to remove it.
    if node.prev == node_addr && node.next == node_addr {
      *list = 0;
      node.prev = 0;
      node.next = 0;
      node.update_checksum();
      return;
    }

    let next = Self::get_slab_node_mut(node.next);
    let prev = Self::get_slab_node_mut(node.prev);

    next.prev = node.prev;
    next.update_checksum();

    prev.next = node.next;
    prev.update_checksum();

    node.next = 0;
    node.prev = 0;
    node.update_checksum();

    // If removing the list head, just make the next node the head.
    if *list == node_addr {
      *list = prev.next;
    }
  }

  /// Get the size of a slab in pages.
  pub fn get_pages_per_slab(&self) -> usize {
    self.slab_pages
  }

  /// Get the number of objects per slab.
  pub fn get_objects_per_slab(&self) -> usize {
    self.objs_per_slab
  }

  /// Get the number of slabs current allocated.
  pub fn get_slabs_allocated(&self) -> usize {
    self.slabs_allocated
  }

  /// Get the number of objects allocated.
  pub fn get_objects_allocated(&self) -> usize {
    self.objects_allocated
  }

  /// Get the number of objects available.
  pub fn get_objects_available(&self) -> usize {
    self.objects_available
  }

  /// Allocate an object from a slab.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator that can allocate new slabs, if needed.
  ///
  /// # Description
  ///
  /// Object allocation uses the following algorithm:
  ///
  /// ```
  /// Alloc:
  ///   If an in-use slab is available:
  ///     Remove an object.
  ///     If the slab is now empty, move it to the empty list.
  ///     Return the object.
  ///   If the unused list is not empty:
  ///     Move an unused slab to the in-use list.
  ///     Goto Alloc.
  ///   If no unused slabs are available:
  ///     Attempt to allocate a slab.
  ///     If successful, place the slab on the in-use list.
  ///     Goto Alloc.
  ///   Return None.
  /// ```
  ///
  /// # Returns
  ///
  /// The virtual address of the new object, or None if unable to allocate.
  pub fn alloc(&mut self, allocator: &SpinLock<A>) -> Option<usize> {
    // If there are no in-use slabs, but there are some unused slabs, take the
    // first unused slab and move it to the in-use list.
    if self.in_use == 0 && self.unused != 0 {
      let tmp_addr = self.unused;
      Self::remove_slab_from_list(&mut self.unused, tmp_addr);
      Self::add_slab_to_tail(&mut self.in_use, tmp_addr);
    }

    // If there are still no in-use slabs, try to allocate one.
    if self.in_use == 0 {
      let tmp_addr = self.alloc_slab(allocator).unwrap_or(0);
      Self::add_slab_to_tail(&mut self.in_use, tmp_addr);
    }

    // If there are still no in-use slabs, fail.
    if self.in_use == 0 {
      return None;
    }

    // Allocate an object from the slab. The free list should not be empty.
    let node = Self::get_slab_node_mut(self.in_use);
    assert_ne!(node.free, 0);
    assert!(node.avail > 0);

    let obj_addr = node.free;
    let obj = Self::get_object_mut(obj_addr);
    node.avail -= 1;
    node.free = obj.next;
    node.update_checksum();
    obj.next = bits::POISON;

    // If all objects have been allocated, move the slab to the empty list.
    if node.avail == 0 {
      let tmp_addr = self.in_use;
      Self::remove_slab_from_list(&mut self.in_use, tmp_addr);
      Self::add_slab_to_tail(&mut self.empty, tmp_addr);
    }

    // Update stats.
    self.objects_allocated += 1;
    self.objects_available -= 1;

    Some(obj_addr)
  }

  /// Release an object back to its slab.
  ///
  /// # Parameters
  ///
  /// * `obj_addr` - The object to free.
  ///
  /// # Description
  ///
  /// A null object address will simply be ignored.
  ///
  /// Freeing an object uses the following algorithm:
  ///
  /// ```
  /// Free:
  ///   Align the object address down to the slab size.
  ///   Add the offset to the slab metadata.
  ///   Verify the checksum of the slab metadata.
  ///   Add the object to the slab's free list.
  ///
  ///   If the slab is on the empty list:
  ///     Move the slab to the in-use list.
  ///     Return.
  ///   If the slab is now full:
  ///     Move the slab to the unused list.
  ///     Return.
  /// ```
  pub fn free(&mut self, obj_addr: usize) {
    // Do nothing if the address is null.
    if obj_addr == 0 {
      return;
    }

    // Calculate the node address from the object address.
    let slab_size = self.slab_pages << arch::get_page_shift();
    let slab_addr = bits::align_down(obj_addr, slab_size);
    let node_addr = slab_addr + slab_size - size_of::<SlabNode>();

    // Get the node and verify the checksum.
    let node = Self::get_slab_node_mut(node_addr);
    assert!(node.avail < self.objs_per_slab);

    // If the slab is empty, move it to the in-use list.
    if node.avail == 0 {
      Self::remove_slab_from_list(&mut self.empty, node_addr);
      Self::add_slab_to_tail(&mut self.in_use, node_addr);
    }

    // Add the object back to the slab's free list. Verify that the next pointer
    // is set to the poison bits to detect a double free or memory overrun.
    let obj = Self::get_object_mut(obj_addr);
    assert_eq!(obj.next, bits::POISON);
    obj.next = node.free;
    node.avail += 1;
    node.free = obj_addr;
    node.update_checksum();

    // If the slab is full, move it to the unused list.
    if node.avail == self.objs_per_slab {
      Self::remove_slab_from_list(&mut self.in_use, node_addr);
      Self::add_slab_to_tail(&mut self.unused, node_addr);
    }

    // Update stats.
    self.objects_allocated -= 1;
    self.objects_available += 1;
  }

  /// Free any unused slabs.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator that can free slabs, if needed.
  pub fn free_unused(&mut self, allocator: &SpinLock<A>) {
    let virt_base = arch::get_kernel_virtual_base();
    let slab_size = self.slab_pages << arch::get_page_shift();
    let mut node_addr = self.unused;

    loop {
      let node = Self::get_slab_node_mut(node_addr);
      node_addr = node.next;

      // Poison the node.
      node.avail = bits::POISON;
      node.free = bits::POISON;
      node.prev = bits::POISON;
      node.next = bits::POISON;
      node.checksum = bits::POISON;

      // Free the slab back to the page allocator.
      let slab_addr = bits::align_down(node_addr, slab_size);
      allocator
        .lock()
        .free(slab_addr - virt_base, self.slab_pages);

      // Update stats.
      self.slabs_allocated -= 1;
      self.objects_available -= self.objs_per_slab;

      // Check for the end of the list.
      if node_addr == self.unused {
        break;
      }
    }

    self.unused = 0;
  }

  /// Allocate a slab and initialize its free list.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator that can allocate new slabs.
  ///
  /// # Returns
  ///
  /// The new slab's node address, or None.
  fn alloc_slab(&mut self, allocator: &SpinLock<A>) -> Option<usize> {
    // Attempt to allocate a slab. We can discard the number of pages actually
    // allocated.
    let Some((phys_addr, _)) = allocator.lock().alloc(self.slab_pages) else {
      return None;
    };

    // Get the node at the end of the slab assuming linear memory.
    let slab_size = self.slab_pages * arch::get_page_size();
    let virt_base = arch::get_kernel_virtual_base();
    let virt_addr = phys_addr + virt_base;
    let node_addr = virt_addr + slab_size - size_of::<SlabNode>();
    let node = Self::get_slab_node_unchecked_mut(node_addr);

    // Initialize the slab's free list and available count.
    node.avail = self.objs_per_slab;
    node.free = 0;
    node.prev = 0;
    node.next = 0;

    let mut obj_addr = virt_addr;

    for _ in 0..self.objs_per_slab {
      let wrapper = Self::get_object_mut(obj_addr);
      wrapper.next = node.free;
      node.free = obj_addr;
      obj_addr += size_of::<ObjectWrapper<T>>();
    }

    node.update_checksum();

    // Update stats.
    self.slabs_allocated += 1;
    self.objects_available += self.objs_per_slab;

    Some(node_addr)
  }
}

/// Give cores up to 16 objects per bundle.
const BUNDLE_SIZE: usize = 16;

/// A bundle is a stack of linear virtual object addresses. Bundles held by the
/// Bundle Manager are either full or empty, so there is no reason to keep the
/// count state with the bundle.
type Bundle = [usize; BUNDLE_SIZE];

/// A bundle wrapper encapsulates a bundle and a pointer to the next wrapper in
/// a free list. The pointer is a linear virtual address.
#[repr(C)]
struct BundleWrapper {
  bundle: Bundle,
  next: usize,
}

/// The Bundle Manager implements the bundle layer. The bundle layer only
/// manages bundles. The Slab Allocator is responsible for allocating the type
/// of object it manages.
///
///   NOTE: The Bundle Manager is NOT thread-safe.
struct BundleManager<A> {
  bundle_alloc: SlabManager<A, BundleWrapper>,
  empty: usize,
  full: usize,
}

impl<'alloc, A> BundleManager<A>
where
  A: PageAllocator,
{
  /// Construct a new Bundle Manager.
  const fn new() -> Self {
    Self {
      bundle_alloc: SlabManager::<A, BundleWrapper>::new(),
      empty: 0,
      full: 0,
    }
  }

  /// Get a bundle wrapper from a bundle address.
  fn get_bundle_wrapper_mut(bundle_addr: usize) -> &'alloc mut BundleWrapper {
    unsafe { (bundle_addr as *mut BundleWrapper).as_mut().unwrap() }
  }

  /// Return a bundle to the manager.
  ///
  /// # Parameters
  ///
  /// * `bundle_addr` - The virtual address of the bundle to return.
  fn return_bundle(bundle_addr: usize, list: &mut usize) {
    if bundle_addr == 0 {
      return;
    }

    let wrapper = Self::get_bundle_wrapper_mut(bundle_addr);
    assert_eq!(wrapper.next, bits::POISON);
    wrapper.next = *list;
    *list = bundle_addr;
  }

  /// Allocate an empty bundle.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator that can allocate new slabs.
  ///
  /// # Description
  ///
  /// This method should only be used to initialize the cache layer of a Slab
  /// Allocator. Once initialized, any new empty bundles must be allocated via
  /// an exchange for a full bundle.
  ///
  /// # Returns
  ///
  /// The virtual address of an empty bundle, or None if one is not available
  /// and one could not be allocated.
  fn alloc_empty_bundle(&mut self, allocator: &SpinLock<A>) -> Option<usize> {
    if self.empty == 0
      && let Some(tmp_addr) = self.bundle_alloc.alloc(allocator)
    {
      let wrapper = Self::get_bundle_wrapper_mut(tmp_addr);
      wrapper.next = 0;
      self.empty = tmp_addr;
    }

    if self.empty == 0 {
      return None;
    }

    let bundle_addr = self.empty;
    let wrapper = Self::get_bundle_wrapper_mut(self.empty);
    self.empty = wrapper.next;
    wrapper.next = bits::POISON;

    Some(bundle_addr)
  }

  /// Exchange an empty bundle for a full bundle.
  ///
  /// # Parameters
  ///
  /// * `bundle_addr` - The virtual address of the empty bundle to exchange.
  ///
  /// # Description
  ///
  /// If the bundle address to exchange is null, the function will only attempt
  /// to get a full bundle.
  ///
  /// If a full bundle is not available, the specified empty bundle is NOT
  /// returned to the Bundle Manager and still available for use by the caller.
  ///
  /// # Returns
  ///
  /// The virtual address of a full bundle, or None if no full bundles are
  /// available.
  fn exchange_empty_bundle(&mut self, bundle_addr: usize) -> Option<usize> {
    if self.full == 0 {
      return None;
    }

    if bundle_addr != 0 {
      Self::return_bundle(bundle_addr, &mut self.empty);
    }

    let ret_addr = self.full;
    let wrapper = Self::get_bundle_wrapper_mut(self.full);
    self.full = wrapper.next;
    wrapper.next = bits::POISON;

    Some(ret_addr)
  }

  /// Exchange a full bundle for an empty bundle.
  ///
  /// # Parameters
  ///
  /// * `bundle_addr` - The virtual address of the full bundle to exchange.
  /// * `allocator` - An allocator that can allocate new slabs.
  ///
  /// # Description
  ///
  /// If the bundle address to exchange is null, the function will only attempt
  /// to allocate a new empty bundle.
  ///
  /// If no empty bundles are available, the function will attempt to allocate
  /// one using the Bundle Manager's Slab Manager.
  ///
  /// # Returns
  ///
  /// The virtual address of an empty bundle, or None if one is not available
  /// and one could not be allocated.
  fn exchange_full_bundle(&mut self, bundle_addr: usize, allocator: &SpinLock<A>) -> Option<usize> {
    if bundle_addr != 0 {
      Self::return_bundle(bundle_addr, &mut self.full);
    }

    self.alloc_empty_bundle(allocator)
  }
}

/// The Core Cache implements the cache for a single core.
struct CoreCache {
  current: usize,
  curr_count: usize,
  standby: usize,
  stby_count: usize,
}

impl CoreCache {
  /// Construct an empty cache.
  const fn new() -> Self {
    Self {
      current: 0,
      curr_count: 0,
      standby: 0,
      stby_count: 0,
    }
  }
}

/// Initialization. Kernel Objects managed by a Slab Allocator must implement
/// this trait.
pub trait Init {
  /// Initialize this object.
  fn init(&mut self) {}
}

/// Deinitialization. Kernel Objects managed by a Slab Allocator must implement
/// this trait.
pub trait Deinit {
  /// Deinitialize this object.
  fn deinit(&mut self) {}
}

/// The Slab Allocator implements the slab allocation algorithm described in
/// BONWICK01.
///
/// The Slab Allocator maintains a core cache for every core in the system. Each
/// Core Cache maintains a current and standby bundle. Both are allocated by the
/// Slab Allocator's Bundle Manager and both are initially empty. The Bundle
/// Manager maintains its own Slab Manager for bundle allocation while the Slab
/// Allocator maintains a Slab Manager for kernel object allocation.
///
/// The process of allocating and freeing kernel objects naturally fills
/// bundles. There is no need to explicitly allocate a bundle and fill it with
/// allocated objects.
///
///   NOTE: The Slab Allocator is intended for allocating kernel objects. The
///         allocator used to allocate slabs MUST allocate from linear memory.
///
///   NOTE: On a 32-bit platform, a Core Cache is 8 bytes and 32-bit platforms
///         are limited to 16 cores. If we allocate 16 cache objects, we are
///         going to allocate a 4 KiB page and use 128 bytes of it. On a 64-bit
///         platform a Core Cache is 16 bytes and 64-bit platforms are currently
///         limited to 256 cores. If we have 256 cores, we would use exactly one
///         page, but that is still a relatively rare situation. More common
///         configurations of 4 to 32 cores would waste most of the page.
///
///   NOTE: A Kernel Object will maintain an immutable reference to the Slab
///         Allocator that manages it. UnsafeCell is used for the core cache
///         array to allow interior mutability. Allocation and free calls will
///         only modify the core cache for the core performing the operation.
///         However, the allocator must prevent the current task from migrating
///         to another core in the middle of an operation that modifies a core
///         cache. UnsafeCell is used instead of RefCell to avoid the type
///         overhead of Ref and MutRef.
pub struct SlabAllocator<'alloc, A, T> {
  cache: UnsafeCell<[CoreCache; cpu::MAX_CORES]>,
  obj_alloc: SpinLock<SlabManager<A, T>>,
  bundle_mgr: SpinLock<BundleManager<A>>,
  allocator: &'alloc SpinLock<A>,
}

impl<'alloc, A, T> SlabAllocator<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  const SINGLE_CACHE_INITIALIZER: CoreCache = CoreCache::new();

  const CACHE_ARRAY_INITIALIZER: UnsafeCell<[CoreCache; cpu::MAX_CORES]> =
    UnsafeCell::new([Self::SINGLE_CACHE_INITIALIZER; cpu::MAX_CORES]);

  const OBJ_ALLOC_INITIALIZER: SpinLock<SlabManager<A, T>> = SpinLock::new(SlabManager::new());

  const BUNDLE_MGR_INITIALIZER: SpinLock<BundleManager<A>> = SpinLock::new(BundleManager::new());

  /// Get a bundle from a bundle address.
  fn get_bundle_mut(bundle_addr: usize) -> &'alloc mut Bundle {
    unsafe { (bundle_addr as *mut Bundle).as_mut().unwrap() }
  }

  /// Helper to allocate from a non-empty cache.
  ///
  /// # Parameters
  ///
  /// * `cache` - A cache with available objects.
  ///
  /// # Assumptions
  ///
  /// The cache's current bundle is valid.
  ///
  /// # Returns
  ///
  /// The virtual address of an object.
  fn alloc_from_cache(cache: &mut CoreCache) -> usize {
    let bundle = Self::get_bundle_mut(cache.current);
    let addr = (*bundle)[cache.curr_count - 1];
    cache.curr_count -= 1;
    addr
  }

  /// Helper to release an object back to a non-full cache.
  ///
  /// # Parameters
  ///
  /// * `cache` - A cache with available space.
  /// * `obj_addr` - The virtual address of the object to release.
  ///
  /// # Assumptions
  ///
  /// The cache's current bundle is valid.
  fn release_to_cache(cache: &mut CoreCache, obj_addr: usize) {
    let bundle = Self::get_bundle_mut(cache.current);
    (*bundle)[cache.curr_count] = obj_addr;
    cache.curr_count += 1;
  }

  /// Swap the cache bundles.
  ///
  /// # Parameters
  ///
  /// * `cache` - The cache to update.
  fn swap_cache_bundles(cache: &mut CoreCache) {
    mem::swap(&mut cache.current, &mut cache.standby);
    mem::swap(&mut cache.curr_count, &mut cache.stby_count);
  }

  /// Construct a new Slab Allocator.
  ///
  /// # Parameters
  ///
  /// * `allocator` - An allocator that can allocate new slabs.
  ///
  /// # Description
  ///
  ///   NOTE: The allocator MUST allocate from linear memory.
  pub const fn new(allocator: &'alloc SpinLock<A>) -> Self {
    let mut slab_alloc = MaybeUninit::<Self>::uninit();
    Self::inplace_new(slab_alloc.as_mut_ptr(), allocator);
    unsafe { slab_alloc.assume_init() }
  }

  /// Construct a new slab allocator in uninitialized memory.
  ///
  /// # Parameters
  ///
  /// * `slab_alloc` - The uninitialized allocator to construct.
  /// * `allocator` - An allocator that can allocate new slabs.
  ///
  /// # Description
  ///
  ///   NOTE: The allocator MUST allocate from linear memory.
  pub const fn inplace_new(slab_alloc: *mut Self, allocator: &'alloc SpinLock<A>) {
    let slab_alloc = unsafe { slab_alloc.as_mut().unwrap() };

    // Leave the Core Caches initialized without any bundles. Bundles will be
    // allocated on demand by the core.
    slab_alloc.cache = Self::CACHE_ARRAY_INITIALIZER;
    slab_alloc.obj_alloc = Self::OBJ_ALLOC_INITIALIZER;
    slab_alloc.bundle_mgr = Self::BUNDLE_MGR_INITIALIZER;
    slab_alloc.allocator = allocator;
  }

  /// Allocate a new kernel object of type T.
  ///
  /// # Returns
  ///
  /// A new, initialized kernel object of type T, or None if unable to allocate
  /// an instance.
  pub fn alloc(&'alloc self) -> Option<KernelObject<'alloc, A, T>> {
    // Mask interrupts for the duration of the internal allocation call. This
    // ensures the current task remains isolated on this core while it is
    // modifying the core cache.
    let irq_state = arch::interrupts::save_and_mask_all_interrupts();
    let ret = self.alloc_internal();
    arch::interrupts::restore_interrupt_state(irq_state);

    let Some(addr) = ret else {
      return None;
    };

    let Some(obj) = (unsafe { (addr as *mut T).as_mut() }) else {
      return None;
    };

    obj.init();

    Some(KernelObject {
      slab_alloc: self,
      obj,
    })
  }

  /// Free an object of type T allocated by this allocator.
  ///
  /// # Parameters
  ///
  /// * `obj` - The object to free.
  ///
  /// # Description
  ///
  ///   NOTE: This method is intentionally private. It should be called by a
  ///         Kernel Object being dropped, not by a client.
  ///
  fn free(&self, obj: &mut T) {
    obj.deinit();

    // Mask interrupts for the duration of the internal free call. This ensures
    // the current task remains isolated on this core while it is modifying the
    // core cache.
    let irq_state = arch::interrupts::save_and_mask_all_interrupts();
    self.free_internal(obj as *const _ as usize);
    arch::interrupts::restore_interrupt_state(irq_state);
  }

  /// Allocate an object of type T.
  ///
  /// # Description
  ///
  /// Implements the following allocation algorithm from BONWICK01:
  ///
  /// ```
  /// Alloc:
  ///   If the core's current bundle is not empty:
  ///     Pop an object address from the bundle.
  ///     Return the object.
  ///   If the core's standby bundle is full:
  ///     Swap the bundles.
  ///     Goto Alloc.
  ///   If the Bundle Manager has a full bundle:
  ///     Exchange empty standby bundle for a full bundle.
  ///     Swap bundles.
  ///     Goto Alloc.
  ///   Allocate a kernel object from the Slab Manager.
  ///   Return the object.
  /// ```
  ///
  /// # Assumptions
  ///
  /// Assumes that interrupts are disabled and the current task is free to
  /// modify the core cache for the current core ONLY.
  ///
  /// # Returns
  ///
  /// The virtual address of a new object of type T, or None if unable to
  /// allocate an instance.
  fn alloc_internal(&self) -> Option<usize> {
    let core_idx = arch::get_current_core_index();
    let cache = &mut unsafe { self.cache.get().as_mut().unwrap() }[core_idx];

    // Lazy allocate the current and standby bundles for the current core. It is
    // possible these allocations will fail, but the allocation algorithm takes
    // that into account.
    if cache.current == 0 {
      cache.current = self
        .bundle_mgr
        .lock()
        .alloc_empty_bundle(self.allocator)
        .unwrap_or(0);
    }

    if cache.standby == 0 {
      cache.standby = self
        .bundle_mgr
        .lock()
        .alloc_empty_bundle(self.allocator)
        .unwrap_or(0);
    }

    // Case 1: We have objects cached in the current bundle. The count is
    // greater than zero, so we can assume the current bundle is valid.
    if cache.curr_count > 0 {
      return Some(Self::alloc_from_cache(cache));
    }

    // Case 2: The standby bundle is not empty. If we get here, the current
    // bundle is empty or invalid. Either way, we can swap the bundles and
    // allocate assuming the current bundle is valid after the swap.
    if cache.stby_count > 0 {
      Self::swap_cache_bundles(cache);
      return Some(Self::alloc_from_cache(cache));
    }

    // Case 3: The Bundle Manager has a full bundle. Both the current and
    // standby bundles are either empty or invalid. Attempt to exchange the
    // current bundle for a full one.
    if let Some(bundle_addr) = self.bundle_mgr.lock().exchange_empty_bundle(cache.current) {
      cache.standby = bundle_addr;
      cache.stby_count = BUNDLE_SIZE;
      Self::swap_cache_bundles(cache);
      return Some(Self::alloc_from_cache(cache));
    }

    // Finally, we need to allocate an object directly if we get here.
    self.obj_alloc.lock().alloc(self.allocator)
  }

  /// Free an object of type T allocated by this allocator.
  ///
  /// # Parameters
  ///
  /// * `obj_addr` - The object to free.
  ///
  /// # Description
  ///
  /// Implements the following algorithm from BONWICK01:
  ///
  /// ```
  /// Free:
  ///   If the core's current bundle is not full:
  ///     Apply the destructor.
  ///     Push the object address to the bundle.
  ///     Return.
  ///   If the core's standby bundle is empty:
  ///     Swap bundles.
  ///     Goto Free.
  ///   If the Bundle Manager has an empty bundle:
  ///     Exchange full standby bundle for an empty bundle.
  ///     Swap bundles.
  ///     Goto Free.
  ///   Release the kernel object to the Slab Manager.
  /// ```
  ///
  ///   NOTE: BONWICK01 has a fourth step in Free that checks if it is possible
  ///         to allocate a bundle. This is covered in the third step. If the
  ///         Bundle Manager does not have an empty bundle cached, it will
  ///         attempt to allocate one and return it.
  ///
  /// In most scenarios, both the current and standby bundles should be valid.
  /// It is possible, however, that either one or both are invalid. In the cases
  /// where a bundle is invalid, just allow the object to be freed directly.
  /// Subsequent allocations will handle allocating new bundles.
  ///
  /// # Assumptions
  ///
  /// Assumes that interrupts are disabled and the current task is free to
  /// modify the core cache for the current core ONLY.
  fn free_internal(&self, obj_addr: usize) {
    assert_ne!(obj_addr, 0);

    let core_idx = arch::get_current_core_index();
    let cache = &mut unsafe { self.cache.get().as_mut().unwrap() }[core_idx];

    // Case 1: The current bundle is not full. We cannot assume the bundle is
    // valid.
    if cache.curr_count < BUNDLE_SIZE && cache.current != 0 {
      Self::release_to_cache(cache, obj_addr);
      return;
    }

    // Case 2: The standby bundle is valid and not full. We cannot assume the
    // bundle is valid.
    if cache.stby_count < BUNDLE_SIZE && cache.standby != 0 {
      Self::swap_cache_bundles(cache);
      Self::release_to_cache(cache, obj_addr);
      return;
    }

    // Case 3: The Bundle Manager has an empty bundle. Both the current and
    // standby bundles are full or invalid. Attempt to exchange the standby
    // bundle, then swap the current and standby bundles.
    if let Some(bundle_addr) = self
      .bundle_mgr
      .lock()
      .exchange_full_bundle(cache.standby, self.allocator)
    {
      cache.standby = bundle_addr;
      cache.stby_count = 0;
      Self::swap_cache_bundles(cache);
    }

    if cache.current != 0 && cache.curr_count < BUNDLE_SIZE {
      Self::release_to_cache(cache, obj_addr);
      return;
    }

    // Finally, we need to free the object directly if we get here.
    self.obj_alloc.lock().free(obj_addr);
  }
}

/// A Kernel Object uniquely owns a dynamically allocated instance of type T and
/// holds a reference to the allocator that allocated the object. Kernel Objects
/// should only be constructed by a Slab Allocator.
pub struct KernelObject<'obj, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  slab_alloc: &'obj SlabAllocator<'obj, A, T>,
  obj: &'obj mut T,
}

impl<'obj, A, T> Deref for KernelObject<'obj, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  type Target = T;

  /// See `Deref::deref()`.
  fn deref(&self) -> &Self::Target {
    self.obj
  }
}

impl<'obj, A, T> DerefMut for KernelObject<'obj, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  /// See `DerefMut::deref_mut()`.
  fn deref_mut(&mut self) -> &mut Self::Target {
    self.obj
  }
}

impl<'obj, A, T> Drop for KernelObject<'obj, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  /// See `Drop::drop()`.
  fn drop(&mut self) {
    self.slab_alloc.free(self.obj);
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut test::TestContext) {
  tests::run_tests(context);
}
