//! Slab Allocator Tests

use super::{
  BUNDLE_SIZE, BundleManager, BundleWrapper, Deinit, Init, KernelObject, ObjectWrapper,
  SlabAllocator, SlabManager, SlabNode,
};
use crate::arch;
use crate::arch::memory::{MemoryRange, MemoryZone, PageAllocator};
use crate::debug_print;
use crate::mm::page_allocator::BuddyPageAllocator;
use crate::support::bits;
use crate::sync::SpinLock;
use crate::test::{self, memory};
use crate::{check_eq, check_neq, check_none, check_not_none, execute_test, mark_fail};
use core::{mem, ptr};

/// A small object for testing the slab allocator.
struct SmallTestObject {
  a: usize,
  b: usize,
  signature: usize,
}

impl Init for SmallTestObject {
  /// See `Init::init()`.
  fn init(&mut self) {
    self.a = 42;
    self.b = 128;
    self.signature = bits::xor_checksum(&[self.a, self.b]);
  }
}

impl Deinit for SmallTestObject {
  /// See `Deinit::deinit()`.
  fn deinit(&mut self) {
    self.a = bits::POISON;
    self.b = bits::POISON;
    self.signature = bits::POISON;
  }
}

/// A large object for testing the slab allocator.
struct LargeTestObject {
  a: usize,
  b: usize,
  signature: usize,
  padding: [usize; 1020],
}

impl Init for LargeTestObject {
  /// See `Init::init()`.
  fn init(&mut self) {
    self.a = 42;
    self.b = 128;
    self.signature = bits::xor_checksum(&[self.a, self.b]);
    self.padding.fill(0xab);
  }
}

impl Deinit for LargeTestObject {
  /// See `Deinit::deinit()`.
  fn deinit(&mut self) {
    self.a = bits::POISON;
    self.b = bits::POISON;
    self.signature = bits::POISON;
    self.padding.fill(bits::POISON);
  }
}

/// The Test Allocator either blocks allocations to simulate low memory, or it
/// passes allocation requests through to a real allocator.
struct TestPageAllocator<A> {
  allocator: A,
  can_alloc: bool,
}

impl<A> TestPageAllocator<A> {
  /// Construct a new Test Allocator.
  pub fn new(allocator: A) -> Self {
    Self {
      allocator,
      can_alloc: true,
    }
  }

  /// Set the allocation pass-through state.
  ///
  /// # Parameters
  ///
  /// * `can_alloc` - Whether the allocator should allow allocations.
  pub fn set_can_alloc(&mut self, can_alloc: bool) {
    self.can_alloc = can_alloc;
  }
}

impl<A> PageAllocator for TestPageAllocator<A>
where
  A: PageAllocator,
{
  /// See `PageAllocator::MAX_BLOCK_PAGES`.
  const MAX_BLOCK_PAGES: usize = A::MAX_BLOCK_PAGES;

  /// See `PageAllocator::alloc()`.
  fn alloc(&mut self, pages: usize) -> Option<(usize, usize)> {
    if !self.can_alloc {
      return None;
    }

    self.allocator.alloc(pages)
  }

  /// See `PageAllocator::free()`.
  fn free(&mut self, addr: usize, pages: usize) {
    self.allocator.free(addr, pages);
  }

  /// See `PageAllocator::get_alloc_mem()`.
  fn get_alloc_mem(&self) -> usize {
    self.allocator.get_alloc_mem()
  }

  /// See `PageAllocator::get_free_mem()`.
  fn get_free_mem(&self) -> usize {
    self.allocator.get_free_mem()
  }
}

/// Uniform allocator convenience type.
type TestAllocator<'alloc> = TestPageAllocator<BuddyPageAllocator<'alloc>>;

/// Test configuration for the generic slab manager tests.
struct TestConfig {
  slab_pages: usize,
  slab_size: usize,
  objs_per_slab: usize,
  addr_list: &'static mut [usize],
}

/// Size of the buddy page allocator metadata.
const META_SIZE: usize = BuddyPageAllocator::calc_metadata_size(memory::MEMORY_SIZE);

/// Use the whole test buffer minus the metadata for the page allocator.
const TEST_MEM_SIZE: usize =
  bits::align_down(memory::MEMORY_SIZE - META_SIZE, arch::get_page_size());

/// The small object slab only needs to be a single page.
const SMALL_SLAB_SIZE: usize = arch::get_page_size();

/// The small object slab page count.
const SMALL_SLAB_PAGES: usize = SMALL_SLAB_SIZE >> arch::get_page_shift();

///              32-bit        64-bit
/// ----------------------------------
/// Page         4 KiB         4 KiB
/// Slab Node    20 B          40 B
/// Small Obj    12 B          24 B
/// Wrapper      16 B          32 B
/// Total Obj    254           126
const SMALL_OBJ_PER_SLAB: usize = (arch::get_page_size() - mem::size_of::<SlabNode>())
  / mem::size_of::<ObjectWrapper<SmallTestObject>>();

/// The large object slab needs to accommodate at least seven objects.
const LARGE_SLAB_PAGES: usize = 1
  << bits::ceil_log2(
    (mem::size_of::<LargeTestObject>() * 7 + mem::size_of::<SlabNode>() + arch::get_page_size()
      - 1)
      / arch::get_page_size(),
  );

/// The large object slab size.
const LARGE_SLAB_SIZE: usize = LARGE_SLAB_PAGES * arch::get_page_size();

///              32-bit        64-bit
/// ----------------------------------
/// Page         4 KiB         4 KiB
/// Slab Node    20 B          40 B
/// Large Obj    4092 B        8184 B
/// Wrapper      4096 B        8192 B
/// Slab         32 KiB        64 KiB
/// Total Obj    7             7
const LARGE_OBJ_PER_SLAB: usize =
  (LARGE_SLAB_SIZE - mem::size_of::<SlabNode>()) / mem::size_of::<ObjectWrapper<LargeTestObject>>();

/// Buffers to keep track of allocated objects. One more than the maximum number
/// of objects will be allocated to force allocation of a second slab.
static mut SMALL_OBJ_ADDRS: [usize; SMALL_OBJ_PER_SLAB + 1] = [0; SMALL_OBJ_PER_SLAB + 1];

static mut LARGE_OBJ_ADDRS: [usize; LARGE_OBJ_PER_SLAB + 1] = [0; LARGE_OBJ_PER_SLAB + 1];

/// Test entry-point.
///
/// # Parameters
///
/// * `context` - The test context.
pub fn run_tests(context: &mut test::TestContext) {
  execute_test!(context, test_small_slab_manager);
  execute_test!(context, test_large_slab_manager);
  execute_test!(context, test_bundle_manager);
  execute_test!(context, test_slab_allocator);
}

/// Test the slab manager with small objects.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_small_slab_manager(context: &mut test::TestContext) {
  let mut config = TestConfig {
    slab_pages: SMALL_SLAB_PAGES,
    slab_size: SMALL_SLAB_SIZE,
    objs_per_slab: SMALL_OBJ_PER_SLAB,
    addr_list: unsafe { &mut *(ptr::addr_of_mut!(SMALL_OBJ_ADDRS)) },
  };

  test_initial_slab_manager_state::<SmallTestObject>(context, &mut config);
  test_slab_manager_single_alloc::<SmallTestObject>(context, &mut config);
  test_slab_manager_fail_alloc::<SmallTestObject>(context, &mut config);
  test_slab_manager_alloc_all::<SmallTestObject>(context, &mut config);
  test_slab_manager_alloc_reuse::<SmallTestObject>(context, &mut config);
  test_slab_manager_alloc_empty::<SmallTestObject>(context, &mut config);
  test_slab_manager_alloc_free_unused::<SmallTestObject>(context, &mut config);
}

/// Test the slab manager with large objects.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_large_slab_manager(context: &mut test::TestContext) {
  let mut config = TestConfig {
    slab_pages: LARGE_SLAB_PAGES,
    slab_size: LARGE_SLAB_SIZE,
    objs_per_slab: LARGE_OBJ_PER_SLAB,
    addr_list: unsafe { &mut *(ptr::addr_of_mut!(LARGE_OBJ_ADDRS)) },
  };

  test_initial_slab_manager_state::<LargeTestObject>(context, &mut config);
  test_slab_manager_single_alloc::<LargeTestObject>(context, &mut config);
  test_slab_manager_fail_alloc::<LargeTestObject>(context, &mut config);
  test_slab_manager_alloc_all::<LargeTestObject>(context, &mut config);
  test_slab_manager_alloc_reuse::<LargeTestObject>(context, &mut config);
  test_slab_manager_alloc_empty::<LargeTestObject>(context, &mut config);
  test_slab_manager_alloc_free_unused::<LargeTestObject>(context, &mut config);
}

/// Verify the initial state of a slab manager.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
fn test_initial_slab_manager_state<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<BuddyPageAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Check metrics.
  check_eq!(context, slab_manager.slab_pages, config.slab_pages);
  check_eq!(context, slab_manager.objs_per_slab, config.objs_per_slab);

  // Check no slabs have been allocated.
  check_eq!(context, slab_manager.unused, 0);
  check_eq!(context, slab_manager.in_use, 0);
  check_eq!(context, slab_manager.empty, 0);
}

/// Verify allocating a single object.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Verify that allocating a single object from a fresh slab manager allocates
/// a slab, places it on the in-use list, and returns a single object.
fn test_slab_manager_single_alloc<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate a single object. Check that the object address is valid and that
  // the manager has an in-use slab.
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  check_neq!(context, addr, 0);
  check_neq!(context, slab_manager.in_use, 0);
  check_eq!(context, slab_manager.unused, 0);
  check_eq!(context, slab_manager.empty, 0);

  // Verify the page allocator has allocated a single slab.
  check_eq!(context, allocator.lock().get_alloc_mem(), config.slab_size);

  // Verify the slab node is valid, points to itself, and indicates one less
  // than the maximum number of objects in a slab are available.
  let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(slab_manager.in_use);
  check_eq!(context, node.verify_checksum(), true);
  check_neq!(context, node.free, 0);
  check_eq!(context, node.prev, slab_manager.in_use);
  check_eq!(context, node.next, slab_manager.in_use);
  check_eq!(context, node.avail, config.objs_per_slab - 1);
}

/// Verify the slab manager gracefully handles memory pressure.
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Provide the slab manager with an allocator that is out of memory and verify
/// it gracefully handles not being able to allocate a slab.
fn test_slab_manager_fail_alloc<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  allocator.lock().set_can_alloc(false);

  // Attempt to allocate an object.
  let addr = slab_manager.alloc(&mut allocator);
  check_none!(context, addr);
}

/// Verify allocating all objects in a slab.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Verify that allocating all objects from a slab moves the slab to the empty
/// list. Verify that allocating another object allocates another slab that is
/// placed on the in-use list.
fn test_slab_manager_alloc_all<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate all but the last object in a slab. There is no need to save the
  // addresses.
  for _ in 0..config.objs_per_slab - 1 {
    let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
    check_neq!(context, addr, 0);
  }

  // Verify the slab has a single object left, it is still on the in-use list,
  // and it is the only one on the list.
  let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(slab_manager.in_use);
  check_eq!(context, node.prev, slab_manager.in_use);
  check_eq!(context, node.next, slab_manager.in_use);
  check_eq!(context, node.avail, 1);

  // Allocate the last object. Verify the slab moves to the empty list and the
  // in-use list is empty.
  let slab_addr = slab_manager.in_use;
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  check_neq!(context, addr, 0);
  check_eq!(context, slab_manager.in_use, 0);
  check_eq!(context, slab_manager.empty, slab_addr);

  // Allocate one more object. Verify that a new slab is allocated and placed on
  // the in-use list.
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  check_neq!(context, addr, 0);
  check_neq!(context, slab_manager.in_use, 0);
  check_neq!(context, slab_manager.in_use, slab_addr);
  check_eq!(context, slab_manager.empty, slab_addr);

  // Verify the page allocator has allocated two slabs.
  check_eq!(context, allocator.lock().get_alloc_mem(), config.slab_size * 2);
}

/// Verify re-using an unused slab.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Verify that an unused slab is re-used when possible rather than allocating
/// a new slab.
fn test_slab_manager_alloc_reuse<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate a single object, then immediately free it.
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  let slab_addr = slab_manager.in_use;
  check_neq!(context, addr, 0);
  check_neq!(context, slab_addr, 0);
  slab_manager.free(addr);

  // Verify that the slab moved to the unused list and that it is the only slab
  // on the list.
  check_eq!(context, slab_manager.unused, slab_addr);
  let node = SlabManager::<TestAllocator, T>::get_slab_node_unchecked_mut(slab_manager.unused);
  check_eq!(context, node.verify_checksum(), true);
  check_eq!(context, node.prev, slab_addr);
  check_eq!(context, node.next, slab_addr);
  check_eq!(context, node.avail, config.objs_per_slab);

  // Allocate another object and verify the slab is reused and placed on the
  // in-use list.
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  check_neq!(context, addr, 0);
  check_eq!(context, slab_manager.in_use, slab_addr);
  check_eq!(context, slab_manager.unused, 0);

  // Verify the slab is the only slab on the in-use list and has one less object
  // available.
  let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(slab_manager.in_use);
  check_eq!(context, node.verify_checksum(), true);
  check_eq!(context, node.prev, slab_addr);
  check_eq!(context, node.next, slab_addr);
  check_eq!(context, node.avail, config.objs_per_slab - 1);

  // Verify the page allocator has allocated one slab.
  check_eq!(context, allocator.lock().get_alloc_mem(), config.slab_size);
}

/// Verify moving from empty to in-use.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Empty a slab, then free an object. Verify the slab moves back to the in-use
/// list.
fn test_slab_manager_alloc_empty<T>(context: &mut test::TestContext, config: &mut TestConfig) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate all but the last object in a slab. There is no need to save the
  // addresses.
  for _ in 0..config.objs_per_slab - 1 {
    let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
    check_neq!(context, addr, 0);
  }

  // Allocate the last object and save off its address.
  let addr = slab_manager.alloc(&mut allocator).unwrap_or(0);
  check_neq!(context, addr, 0);
  check_eq!(context, slab_manager.in_use, 0);
  check_neq!(context, slab_manager.empty, 0);

  // Verify the slab's node.
  let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(slab_manager.empty);
  check_eq!(context, node.verify_checksum(), true);
  check_eq!(context, node.prev, slab_manager.empty);
  check_eq!(context, node.next, slab_manager.empty);
  check_eq!(context, node.free, 0);
  check_eq!(context, node.avail, 0);

  // Free the object and verify the slab moves back to the in-use list.
  let slab_addr = slab_manager.empty;
  slab_manager.free(addr);
  check_eq!(context, slab_manager.in_use, slab_addr);
  check_eq!(context, slab_manager.empty, 0);

  // Verify the slab's node.
  let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(slab_manager.in_use);
  check_eq!(context, node.verify_checksum(), true);
  check_eq!(context, node.prev, slab_manager.in_use);
  check_eq!(context, node.next, slab_manager.in_use);
  check_neq!(context, node.free, 0);
  check_eq!(context, node.avail, 1);

  // Verify the page allocator has allocated one slab.
  check_eq!(context, allocator.lock().get_alloc_mem(), config.slab_size);
}

/// Verify freeing unused slabs.
///
/// # Parameters
///
/// * `context` - The test context.
/// * `config` - The test configuration.
///
/// # Description
///
/// Allocate enough objects to bring two slabs into existence, free all objects
/// to put both slabs on the unused list, then free the unused slabs.
fn test_slab_manager_alloc_free_unused<T>(
  context: &mut test::TestContext,
  config: &mut TestConfig,
) {
  let mut slab_manager = SlabManager::<TestAllocator, T>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate one more than the number of objects in a slab to bring two slabs
  // into existence.
  for i in 0..config.objs_per_slab + 1 {
    config.addr_list[i] = slab_manager.alloc(&mut allocator).unwrap_or(0);
    check_neq!(context, config.addr_list[i], 0);
  }

  check_eq!(context, slab_manager.unused, 0);
  check_neq!(context, slab_manager.in_use, 0);
  check_neq!(context, slab_manager.empty, 0);

  // Free all objects and verify both slabs are on the unused list.
  let slab_a = slab_manager.empty;
  let slab_b = slab_manager.in_use;
  for i in 0..config.objs_per_slab + 1 {
    slab_manager.free(config.addr_list[i]);
  }

  check_neq!(context, slab_manager.unused, 0);
  check_eq!(context, slab_manager.in_use, 0);
  check_eq!(context, slab_manager.empty, 0);

  // Verify both slabs are on the unused list.
  let mut seen = 0;
  let mut addr = slab_manager.unused;

  loop {
    let node = SlabManager::<BuddyPageAllocator, T>::get_slab_node_unchecked_mut(addr);

    check_eq!(context, node.verify_checksum(), true);

    match addr {
      slab_a => seen += 1,
      slab_b => seen += 1,
      _ => {
        mark_fail!(context, "Unknown slab address.");
      }
    }

    addr = node.next;

    if addr == slab_manager.unused {
      break;
    }
  }

  check_eq!(context, seen, 2);

  // Verify the page allocator has allocated two slabs.
  check_eq!(context, allocator.lock().get_alloc_mem(), config.slab_size * 2);

  // Free the unused slabs and verify they have been deallocated.
  slab_manager.free_unused(&mut allocator);
  check_eq!(context, slab_manager.unused, 0);
  check_eq!(context, slab_manager.in_use, 0);
  check_eq!(context, slab_manager.empty, 0);

  // Verify the page allocator has freed the slabs.
  check_eq!(context, allocator.lock().get_alloc_mem(), 0);
}

/// Run bundle manager tests.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bundle_manager(context: &mut test::TestContext) {
  test_bundle_manager_initial_state(context);
  test_bundle_manager_alloc_empty(context);
  test_bundle_manager_alloc_reuse(context);
  test_bundle_manager_fail_alloc(context);
  test_bundle_manager_exchange_full(context);
  test_bundle_manager_exchange_full_fail(context);
  test_bundle_manager_exchange_empty(context);
}

/// Verify the bundle manager's initial state.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bundle_manager_initial_state(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<BuddyPageAllocator>::new();
  check_eq!(context, bundle_manager.empty, 0);
  check_eq!(context, bundle_manager.full, 0);
}

/// Verify that the bundle manager can allocate empty bundles.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bundle_manager_alloc_empty(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Verify the bundle manager can allocate a bundle.
  let bundle = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle, 0);
}

/// Verify the bundle manager reuses an empty bundle when allocating.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_bundle_manager_alloc_reuse(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Verify the bundle manager can allocate a bundle and that the full and empty
  // lists are empty.
  let bundle_a = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle_a, 0);
  check_eq!(context, bundle_manager.empty, 0);
  check_eq!(context, bundle_manager.full, 0);

  // Exchange the bundle as empty. We expect to get nothing back.
  let bundle_b = bundle_manager.exchange_empty_bundle(bundle_a);
  check_none!(context, bundle_b);

  // Manually put the bundle back into the empty list, then attempt to allocate
  // an empty bundle. Verify we get the same one back.
  BundleManager::<BuddyPageAllocator>::return_bundle(bundle_a, &mut bundle_manager.empty);
  let bundle_b = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_eq!(context, bundle_a, bundle_b);
}

/// Verify the bundle manager gracefully handles memory pressure.
///
/// * `context` - The test context.
///
/// # Description
///
/// Provide the bundle manager with an allocator that is out of memory and
/// verify it gracefully handles not being able to allocate a bundle.
fn test_bundle_manager_fail_alloc(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  allocator.lock().set_can_alloc(false);

  let bundle = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_eq!(context, bundle, 0);
}

/// Verify the bundle manager exchange for full bundles.
///
/// * `context` - The test context.
///
/// # Description
///
/// Verify that the exchange mechanism allocates empty bundles to exchange for
/// full ones when none are available.
fn test_bundle_manager_exchange_full(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate and empty bundle.
  let bundle_a = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle_a, 0);
  check_eq!(context, bundle_manager.empty, 0);
  check_eq!(context, bundle_manager.full, 0);

  // Exchange the "full" bundle for an empty one. This should allocate a second
  // bundle.
  let bundle_b = bundle_manager
    .exchange_full_bundle(bundle_a, &mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle_b, 0);
  check_neq!(context, bundle_b, bundle_a);
  check_eq!(context, bundle_manager.empty, 0);
  check_eq!(context, bundle_manager.full, bundle_a);

  // Artificially move bundle A to the empty list and exchange bundle B as a
  // "full" bundle to verify A is reused.
  bundle_manager.empty = bundle_manager.full;
  bundle_manager.full = 0;
  let bundle_c = bundle_manager
    .exchange_full_bundle(bundle_b, &mut allocator)
    .unwrap_or(0);
  check_eq!(context, bundle_c, bundle_a);
  check_eq!(context, bundle_manager.empty, 0);
  check_eq!(context, bundle_manager.full, bundle_b);

  // Exchange bundle A back as a full bundle and verify both A and B are on the
  // full list.
  let bundle_d = bundle_manager
    .exchange_full_bundle(bundle_c, &mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle_d, 0);

  let mut seen = 0;
  let mut addr = bundle_manager.full;

  while addr != 0 {
    let wrapper = BundleManager::<TestAllocator>::get_bundle_wrapper_mut(addr);

    match addr {
      bundle_a => seen += 1,
      bundle_b => seen += 1,
      _ => {
        mark_fail!(context, "Unknown bundle address.");
      }
    }

    addr = wrapper.next;
  }

  check_eq!(context, seen, 2);
}

/// Verify exchanging a full bundle fails gracefully under memory pressure.
///
/// * `context` - The test context.
///
/// # Description
///
/// Allocate a bundle wrapper on the stack to work around the null allocator,
/// then attempt to exchange it as full to verify the exchange returns None.
fn test_bundle_manager_exchange_full_fail(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());
  let wrapper = BundleWrapper {
    bundle: [0; BUNDLE_SIZE],
    next: bits::POISON,
  };
  let wrapper_addr = ptr::addr_of!(wrapper) as usize;

  allocator.lock().set_can_alloc(false);

  // Expect allocation of a new empty bundle to fail.
  let bundle = bundle_manager.exchange_full_bundle(wrapper_addr, &mut allocator);
  check_none!(context, bundle);

  // Verify the "full" bundle is still added to the full list.
  check_eq!(context, bundle_manager.full, wrapper_addr);
}

/// Verify the exchange for empty bundles.
///
/// * `context` - The test context.
///
/// # Description
///
/// Verify that exchanging an empty bundle returns None when no full bundles are
/// available, and returns a full bundle when one is available.
fn test_bundle_manager_exchange_empty(context: &mut test::TestContext) {
  let mut bundle_manager = BundleManager::<TestAllocator>::new();
  let mut allocator = SpinLock::new(make_page_allocator());

  // Allocate some bundles.
  let bundle_a = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  let bundle_b = bundle_manager
    .alloc_empty_bundle(&mut allocator)
    .unwrap_or(0);
  check_neq!(context, bundle_a, 0);
  check_neq!(context, bundle_b, 0);

  // Artificially place bundle B on the full list.
  let wrapper = BundleManager::<TestAllocator>::get_bundle_wrapper_mut(bundle_b);
  wrapper.next = 0;
  bundle_manager.full = bundle_b;

  // Exchange bundle A as empty and verify we get bundle B back.
  let bundle_c = bundle_manager.exchange_empty_bundle(bundle_a).unwrap_or(0);
  check_eq!(context, bundle_c, bundle_b);
  check_eq!(context, bundle_manager.empty, bundle_a);
  check_eq!(context, bundle_manager.full, 0);

  // Exchange bundle B as empy and verify we get None back. Bundle B will still
  // be valid for use.
  let bundle_d = bundle_manager.exchange_empty_bundle(bundle_c);
  check_none!(context, bundle_d);

  // Verify bundle A is still on the empty list and the full list is empty.
  let wrapper = BundleManager::<TestAllocator>::get_bundle_wrapper_mut(bundle_a);
  check_eq!(context, wrapper.next, 0);
  check_eq!(context, bundle_manager.empty, bundle_a);
  check_eq!(context, bundle_manager.full, 0);
}

/// Run slab allocator tests.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_slab_allocator(context: &mut test::TestContext) {
  test_slab_allocator_initial_state(context);
  test_slab_allocator_initial_alloc_fail(context);
  test_slab_allocator_bundle_alloc(context);
  test_slab_allocator_cache_alloc(context);
  test_slab_allocator_bundle_swap_alloc(context);
  test_slab_allocator_bundle_exchange_alloc(context);
  test_slab_allocator_bundle_cache_free(context);
  test_slab_allocator_bundle_swap_free(context);
  test_slab_allocator_bundle_exchange_free(context);
  test_slab_allocator_bundle_direct_free(context);
  test_slab_allocator_obj_init_deinit(context);
}

/// Allocates an uninitialized bundle.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
///
/// # Returns
///
/// The address of the new bundle.
fn alloc_bundle(slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>) -> usize {
  slab_alloc
    .bundle_mgr
    .lock()
    .alloc_empty_bundle(&slab_alloc.allocator)
    .unwrap()
}

/// Allocates a bundle initialized to zero.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
///
/// # Returns
///
/// The address of the new bundle.
fn alloc_zeroed_bundle(slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>) -> usize {
  let bundle_addr = alloc_bundle(slab_alloc);
  let bundle = SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(bundle_addr);
  bundle.fill(0);
  bundle_addr
}

/// Allocates a bundle initialized with fake object addresses.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
/// * `first` - The first fake address to use.
///
/// # Returns
///
/// The address of the new bundle.
fn alloc_full_bundle(
  slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>,
  first_addr: usize,
) -> usize {
  let bundle_addr = alloc_bundle(slab_alloc);
  let bundle = SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(bundle_addr);
  for i in 0..BUNDLE_SIZE {
    bundle[i] = first_addr + i;
  }
  bundle_addr
}

/// Initialize a core cache with empty current and standby bundles.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
/// * `core_idx` - The core cache to initialize.
fn init_cache_with_empty_bundles(
  slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>,
  core_idx: usize,
) {
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };
  let mut cache = &mut cache_array[core_idx];
  cache.current = alloc_zeroed_bundle(slab_alloc);
  cache.curr_count = 0;
  cache.standby = alloc_zeroed_bundle(slab_alloc);
  cache.stby_count = 0;
}

/// Initialize a core cache with full current and standby bundles.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
/// * `core_idx` - The core cache to initialize.
///
/// # Description
///
/// The current bundle will contain fake addresses [1, BUNDLE_SIZE] and the
/// standby bundle will contain [BUNDLE_SIZE + 1, 2 * BUNDLE_SIZE].
fn init_cache_with_full_bundles(
  slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>,
  core_idx: usize,
) {
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };
  let mut cache = &mut cache_array[core_idx];
  cache.current = alloc_full_bundle(slab_alloc, 1);
  cache.curr_count = BUNDLE_SIZE;
  cache.standby = alloc_full_bundle(slab_alloc, 1 + BUNDLE_SIZE);
  cache.stby_count = BUNDLE_SIZE;
}

/// Initialize a core cache with full current bundle.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
/// * `core_idx` - The core cache to initialize.
///
/// # Description
///
/// The current bundle will contain fake addresses [1, BUNDLE_SIZE].
fn init_cache_with_full_current_bundle(
  slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>,
  core_idx: usize,
) {
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };
  let mut cache = &mut cache_array[core_idx];
  cache.current = alloc_full_bundle(slab_alloc, 1);
  cache.curr_count = BUNDLE_SIZE;
  cache.standby = alloc_zeroed_bundle(slab_alloc);
  cache.stby_count = 0;
}

/// Initialize a core cache with full standby bundle.
///
/// # Parameters
///
/// * `slab_alloc` - The slab allocator to use for allocation.
/// * `core_idx` - The core cache to initialize.
///
/// # Description
///
/// The standby bundle will contain fake addresses
/// [BUNDLE_SIZE + 1, 2 * BUNDLE_SIZE].
fn init_cache_with_full_standby_bundle(
  slab_alloc: &SlabAllocator<TestAllocator, SmallTestObject>,
  core_idx: usize,
) {
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };
  let mut cache = &mut cache_array[core_idx];
  cache.current = alloc_zeroed_bundle(slab_alloc);
  cache.curr_count = 0;
  cache.standby = alloc_full_bundle(slab_alloc, BUNDLE_SIZE + 1);
  cache.stby_count = BUNDLE_SIZE;
}

/// Verify the initial state of a slab allocator.
///
/// * `context` - The test context.
fn test_slab_allocator_initial_state(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Verify no bundles have been allocated for the cores.
  for cache in cache_array {
    check_eq!(context, cache.curr_count, 0);
    check_eq!(context, cache.current, 0);
    check_eq!(context, cache.stby_count, 0);
    check_eq!(context, cache.standby, 0);
  }
}

/// Verify allocation fails if the page allocator fails.
///
/// * `context` - The test context.
fn test_slab_allocator_initial_alloc_fail(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };
  let core_idx = arch::get_current_core_index();

  allocator.lock().set_can_alloc(false);
  let obj = slab_alloc.alloc_internal();
  check_none!(context, obj);

  check_eq!(context, cache_array[core_idx].curr_count, 0);
  check_eq!(context, cache_array[core_idx].current, 0);
  check_eq!(context, cache_array[core_idx].stby_count, 0);
  check_eq!(context, cache_array[core_idx].standby, 0);
}

/// Verify the Slab Allocator allocates bundles on initial allocation.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_alloc(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Allocate an object. Verify the object came directly from the object
  // allocator.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);
  check_eq!(context, slab_alloc.obj_alloc.lock().get_objects_allocated(), 1);

  // Verify bundles have been allocated for the primary core.
  check_eq!(context, cache_array[0].curr_count, 0);
  check_neq!(context, cache_array[0].current, 0);
  check_eq!(context, cache_array[0].stby_count, 0);
  check_neq!(context, cache_array[0].standby, 0);
}

/// Verify the Slab Allocator allocates from the cache when able.
///
/// * `context` - The test context.
fn test_slab_allocator_cache_alloc(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  init_cache_with_full_bundles(slab_alloc, 0);

  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Allocate an object. The allocator should have two full bundles and the top
  // fake address in the current bundle should be BUNDLE_SIZE.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);

  // Verify the current bundle has one less object and the standby bundle is
  // still full.
  check_eq!(context, cache_array[0].curr_count, BUNDLE_SIZE - 1);
  check_eq!(context, cache_array[0].stby_count, BUNDLE_SIZE);
}

/// Verify the Slab Allocator swaps bundles when allocating objects.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_swap_alloc(context: &mut test::TestContext) {
  // See `init_cache_with_full_standby_bundle()`.
  const EXPECTED_FAKE_ADDR: usize = 2 * BUNDLE_SIZE;

  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  init_cache_with_full_standby_bundle(slab_alloc, 0);

  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Save the current and standby bundle addresses.
  let old_current = cache_array[0].current;
  let old_standby = cache_array[0].standby;

  // Allocate an object.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);
  let obj_addr = obj.unwrap();

  // Verify the bundle addresses swapped and that the object came from the now
  // current bundle.
  check_eq!(context, cache_array[0].current, old_standby);
  check_eq!(context, cache_array[0].curr_count, BUNDLE_SIZE - 1);
  check_eq!(context, cache_array[0].standby, old_current);
  check_eq!(context, cache_array[0].stby_count, 0);
  check_eq!(context, obj_addr, EXPECTED_FAKE_ADDR);
}

/// Verify the Slab Allocator exchanges bundles when allocating objects.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_exchange_alloc(context: &mut test::TestContext) {
  // See `alloc_full_bundle()`.
  const EXPECTED_FAKE_ADDR: usize = BUNDLE_SIZE;

  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  init_cache_with_empty_bundles(slab_alloc, 0);

  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Artificially add a full bundle.
  let full_bundle = alloc_full_bundle(slab_alloc, 1);
  let mut bundle_mgr = slab_alloc.bundle_mgr.lock();
  BundleManager::<TestAllocator>::return_bundle(full_bundle, &mut bundle_mgr.full);
  drop(bundle_mgr);

  // Save the current and standby bundle addresses.
  let old_current = cache_array[0].current;
  let old_standby = cache_array[0].standby;

  // Allocate an object.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);
  let obj_addr = obj.unwrap();

  // Verify the bundle addresses swapped and that the object came from the now
  // current bundle.
  check_eq!(context, cache_array[0].current, full_bundle);
  check_eq!(context, cache_array[0].curr_count, BUNDLE_SIZE - 1);
  check_eq!(context, cache_array[0].standby, old_current);
  check_eq!(context, cache_array[0].stby_count, 0);
  check_eq!(context, obj_addr, EXPECTED_FAKE_ADDR);
}

/// Verify the Slab Allocator frees to the cache when able.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_cache_free(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Allocate an object. Verify the object came directly from the object
  // allocator.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);
  check_eq!(context, slab_alloc.obj_alloc.lock().get_objects_allocated(), 1);
  let obj_addr = obj.unwrap();

  // Save the current and standby bundle addresses.
  let old_current = cache_array[0].current;
  let old_standby = cache_array[0].standby;

  // Free a fake address.
  slab_alloc.free_internal(obj_addr);

  // Verify the bundles were NOT swapped, the object is on the current bundle,
  // and the object was not freed.
  let bundle =
    SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(cache_array[0].current);
  check_eq!(context, cache_array[0].current, old_current);
  check_eq!(context, cache_array[0].curr_count, 1);
  check_eq!(context, cache_array[0].standby, old_standby);
  check_eq!(context, cache_array[0].stby_count, 0);
  check_eq!(context, bundle[0], obj_addr);
  check_eq!(context, slab_alloc.obj_alloc.lock().get_objects_allocated(), 1);
}

/// Verify the Slab Allocator swaps bundles when freeing objects.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_swap_free(context: &mut test::TestContext) {
  // See `init_cache_with_full_current_bundle()`.
  const EXPECTED_FAKE_ADDR: usize = BUNDLE_SIZE + 1;

  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  init_cache_with_full_current_bundle(slab_alloc, 0);

  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Save the current and standby bundle addresses.
  let old_current = cache_array[0].current;
  let old_standby = cache_array[0].standby;

  // Free a fake address.
  slab_alloc.free_internal(EXPECTED_FAKE_ADDR);

  // Verify the bundle addresses swapped and that the fake address is in the
  // now current bundle.
  let bundle =
    SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(cache_array[0].current);
  check_eq!(context, cache_array[0].current, old_standby);
  check_eq!(context, cache_array[0].curr_count, 1);
  check_eq!(context, cache_array[0].standby, old_current);
  check_eq!(context, cache_array[0].stby_count, BUNDLE_SIZE);
  check_eq!(context, bundle[0], EXPECTED_FAKE_ADDR);
}

/// Verify the Slab Allocator exchanges bundles when freeing objects.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_exchange_free(context: &mut test::TestContext) {
  // See `init_cache_with_full_bundles()`.
  const EXPECTED_FAKE_ADDR: usize = 2 * BUNDLE_SIZE + 1;

  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  init_cache_with_full_bundles(slab_alloc, 0);

  let cache_array = unsafe { slab_alloc.cache.get().as_ref().unwrap() };

  // Artificially add an empty bundle.
  let empty_bundle = alloc_zeroed_bundle(slab_alloc);
  let mut bundle_mgr = slab_alloc.bundle_mgr.lock();
  BundleManager::<TestAllocator>::return_bundle(empty_bundle, &mut bundle_mgr.empty);
  drop(bundle_mgr);

  // Save the current and standby bundle addresses.
  let old_current = cache_array[0].current;
  let old_standby = cache_array[0].standby;

  // Free a fake object.
  slab_alloc.free_internal(EXPECTED_FAKE_ADDR);

  // Verify the bundle addresses swapped and that the object is in the now
  // current bundle.
  let bundle =
    SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(cache_array[0].current);
  check_eq!(context, cache_array[0].current, empty_bundle);
  check_eq!(context, cache_array[0].curr_count, 1);
  check_eq!(context, cache_array[0].standby, old_current);
  check_eq!(context, cache_array[0].stby_count, BUNDLE_SIZE);
  check_eq!(context, bundle[0], EXPECTED_FAKE_ADDR);
}

/// Verify the Slab Allocator directly frees an object.
///
/// * `context` - The test context.
fn test_slab_allocator_bundle_direct_free(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };

  // Allocate an object and the initial bundles.
  let obj = slab_alloc.alloc_internal();
  check_not_none!(context, obj);
  let obj_addr = obj.unwrap();

  // Both bundles are initially empty, so the object has to come from the object
  // allocator directly.
  check_eq!(context, slab_alloc.obj_alloc.lock().get_objects_allocated(), 1);

  // Artificially set the bundles as full and prevent allocating new bundles.
  // Note that we have to allocate all bundles cached on the bundle manager's
  // current slab to ensure an empty bundle cannot be allocated.
  cache_array[0].curr_count = BUNDLE_SIZE;
  cache_array[0].stby_count = BUNDLE_SIZE;

  allocator.lock().set_can_alloc(false);

  loop {
    let Some(_) = slab_alloc.bundle_mgr.lock().alloc_empty_bundle(&allocator) else {
      break;
    };
  }

  // Free the object. Since both bundles are full and the slab allocator is not
  // able to allocate a new empty bundle, the object must be freed directly by
  // the object allocator.
  slab_alloc.free_internal(obj_addr);
  check_eq!(context, slab_alloc.obj_alloc.lock().get_objects_allocated(), 0);
}

/// Verify the Slab Allocator initializes and deinitializes objects.
///
/// * `context` - The test context.
fn test_slab_allocator_obj_init_deinit(context: &mut test::TestContext) {
  let mut allocator = SpinLock::new(make_page_allocator());
  let slab_alloc = make_slab_allocator::<TestAllocator, SmallTestObject>(&allocator);
  let cache_array = unsafe { slab_alloc.cache.get().as_mut().unwrap() };

  // Allocate an object with the public interface.
  let obj = slab_alloc.alloc();
  check_not_none!(context, obj);

  // Verify the fields were initialized and save the pointer to the object.
  let kobj = obj.unwrap();
  let obj_addr = kobj.obj as *const _ as usize;
  check_eq!(context, kobj.a, 42);
  check_eq!(context, kobj.b, 128);
  check_eq!(context, kobj.signature, bits::xor_checksum(&[42, 128]));

  // Drop the kernel object to deallocate.
  drop(kobj);

  // Verify the fields were deinitialized. The object should be in the core's
  // cache and still valid.
  let bundle =
      SlabAllocator::<TestAllocator, SmallTestObject>::get_bundle_mut(cache_array[0].current);
  check_eq!(context, bundle[0], obj_addr);
  
  let kobj = unsafe { (obj_addr as *const SmallTestObject).as_ref().unwrap() };
  check_eq!(context, kobj.a, bits::POISON);
  check_eq!(context, kobj.b, bits::POISON);
  check_eq!(context, kobj.signature, bits::POISON);
}

/// Construct a test allocator.
///
/// # Description
///
/// Constructs a test allocator with a single available region.
///
///     |----------- MEMORY_SIZE ------------|
///
///            TEST_MEM_SIZE
///     +-------------------------+----------+
///     | Available Region        | Metadata |
///     +-------------------------+----------+
///
/// # Returns
///
/// The new allocator.
fn make_page_allocator<'alloc>() -> TestAllocator<'alloc> {
  let virt_base = arch::get_kernel_virtual_base();
  let phys_addr = memory::get_test_memory_mut().as_ptr() as usize - virt_base;
  let meta_addr = virt_base + phys_addr + TEST_MEM_SIZE;

  memory::reset_test_memory();

  let avail = &[MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: phys_addr,
    size: TEST_MEM_SIZE,
  }];

  // Assume this will never fail. If it does, something is wrong with the test
  // setup.
  TestAllocator::new(
    BuddyPageAllocator::new(phys_addr, TEST_MEM_SIZE, meta_addr as *mut u8, avail).unwrap(),
  )
}

/// Dynamically allocates a slab allocator.
///
/// # Parameters
///
/// * `allocator` - An allocator that can allocate pages.
///
/// # Details
///
/// The slab allocator will be quite large, so it is not possible to allocate
/// one on the stack. Dynamically allocate a slab allocator through a page
/// allocator using linear memory. We do not need to be concerned with how many
/// pages are actually allocated because we are never going to free it.
///
/// # Returns
///
/// A reference to a new slab allocator.
fn make_slab_allocator<'alloc, A, T>(
  allocator: &'alloc SpinLock<A>,
) -> &'alloc SlabAllocator<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized + Init + Deinit,
{
  let virt_base = arch::get_kernel_virtual_base();
  let page_size = arch::get_page_size();
  let pages = (mem::size_of::<SlabAllocator<'alloc, A, T>>() + page_size - 1) / page_size;
  let (base_addr, _) = allocator.lock().alloc(pages).unwrap();
  let slab_alloc = (base_addr + virt_base) as *mut SlabAllocator<'alloc, A, T>;

  SlabAllocator::inplace_new(slab_alloc, allocator);

  unsafe { slab_alloc.as_ref().unwrap() }
}
