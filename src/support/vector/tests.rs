//! Vector Tests

use crate::arch::memory::PageAllocator;
use crate::support::{bits, vector::Vector};
use crate::sync::SpinLock;
use crate::test;
use crate::test::memory::{TestPageAllocator, make_test_page_allocator, reset_test_memory};
use crate::{arch, debug_print};
use crate::{check, check_eq, check_none, check_not_none, execute_test};
use core::slice;

/// A relatively large test structure that keeps capacity small so that we do
/// not need to deal with hundreds of objects in test vectors.
#[derive(Clone)]
struct TestStruct {
  a: [u8; 511],
}

impl TestStruct {
  /// Construct a new test structure filled with the specified byte.
  ///
  /// # Parameters
  ///
  /// * `n` - Fill byte.
  pub fn new(n: u8) -> Self {
    Self { a: [n; 511] }
  }
}

impl Drop for TestStruct {
  /// See `Drop::drop()`.
  fn drop(&mut self) {
    self.a.fill(bits::POISON_BYTE);
  }
}

/// Run the Vector tests.
///
/// # Parameters
///
/// * `context` - The test context.
pub fn run_tests(context: &mut test::TestContext) {
  execute_test!(context, vector, test_initialization);
  execute_test!(context, vector, test_reserve);
  execute_test!(context, vector, test_reserve_alloc_fail);
  execute_test!(context, vector, test_resize);
  execute_test!(context, vector, test_resize_alloc_fail);
  execute_test!(context, vector, test_truncate_to_zero);
  execute_test!(context, vector, test_shrink_to_fit);
  execute_test!(context, vector, test_push_pop);
  execute_test!(context, vector, test_push_alloc_fail);
  execute_test!(context, vector, test_insert);
  execute_test!(context, vector, test_insert_alloc_fail);
  execute_test!(context, vector, test_remove);
}

/// Calculate the page counts and capacity for a number of objects.
///
/// # Parameters
///
/// * `obj_count` - The number of objects.
/// * `obj_size` - The size of each object.
///
/// # Returns
///
/// Returns a tuple with:
///
/// * The minimum required number of pages.
/// * The expected number of pages that will be allocated.
/// * The expected object capacity of the allocated pages.
fn calc_pages_and_capacity(obj_count: usize, obj_size: usize) -> (usize, usize, usize) {
  let page_size = arch::get_page_size();
  let req_pages = (obj_count * obj_size + page_size - 1) / page_size;
  let exp_pages = 1 << bits::ceil_log2(req_pages);
  let exp_capacity = (exp_pages * page_size) / obj_size;

  (req_pages, exp_pages, exp_capacity)
}

/// Verify a block of memory has been filled with the specified byte.
///
/// # Parameters
///
///
/// * `ptr` - A pointer to valid memory.
/// * `offset` - An offset to the start of the memory block.
/// * `len` - The length of the block.
/// * `expected` - The fill byte.
fn verify_memory(ptr: *const u8, offset: usize, len: usize, expected: u8) -> bool {
  let mem = unsafe { slice::from_raw_parts(ptr.add(offset), len) };

  for b in mem {
    if *b != expected {
      return false;
    }
  }

  true
}

/// Verify the vector initializes without allocating memory.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_initialization(context: &mut test::TestContext) {
  let allocator = SpinLock::new(make_test_page_allocator());
  let free_mem = allocator.lock().get_free_mem();
  let v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, 0);
  check_eq!(context, v.pages, 0);
  check_eq!(context, allocator.lock().get_free_mem(), free_mem);
}

/// Verify reserving memory.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_reserve(context: &mut test::TestContext) {
  const RSV_COUNT1: usize = 5;
  const RSV_COUNT2: usize = 10;

  let allocator = SpinLock::new(make_test_page_allocator());
  let free_mem = allocator.lock().get_free_mem();
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Reserve space for five objects and verify the states of the vector and the
  // allocator. The allocator should have performed one allocation for
  // `req_pages1`.
  let page_size = arch::get_page_size();
  let (_, exp_pages1, exp_capacity) = calc_pages_and_capacity(RSV_COUNT1, size_of::<TestStruct>());

  v.reserve(RSV_COUNT1);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages1);
  {
    let lock = allocator.lock();
    check_eq!(context, lock.get_free_mem(), free_mem - exp_pages1 * page_size);
    check_eq!(context, lock.get_free_count(), 1);
    check_eq!(context, lock.get_free_total(), 0);
    check_eq!(context, lock.get_alloc_count(), 1);
    check_eq!(context, lock.get_alloc_total(), exp_pages1);
  }

  // Reserve space for 10 objects. Verify the capacity of the vector. Verify
  // that `exp_pages1` were freed and `req_pages2` were allocated.
  let (_, exp_pages2, exp_capacity) = calc_pages_and_capacity(RSV_COUNT2, size_of::<TestStruct>());

  v.reserve(RSV_COUNT2);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages2);
  {
    let lock = allocator.lock();
    check_eq!(context, lock.get_free_mem(), free_mem - exp_pages2 * page_size);
    check_eq!(context, lock.get_free_count(), 2);
    check_eq!(context, lock.get_free_total(), exp_pages1);
    check_eq!(context, lock.get_alloc_count(), 2);
    check_eq!(context, lock.get_alloc_total(), exp_pages1 + exp_pages2);
  }

  drop(v);
  check_eq!(context, allocator.lock().get_free_mem(), free_mem);
}

/// Verify that the vector is unmodified if allocation fails during a reserve.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_reserve_alloc_fail(context: &mut test::TestContext) {
  const RSV_COUNT: usize = 10;

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Freeze the allocator before the vector can allocate any memory. Verify its
  // length, capacity, and page count remain 0.
  allocator.lock().set_can_alloc(false);
  v.reserve(RSV_COUNT);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, 0);
  check_eq!(context, v.pages, 0);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(RSV_COUNT, size_of::<TestStruct>());

  // Unfreeze the allocator and allow the vector to reserve at least RSV_COUNT
  // elements. Verify the length 0, but the capacity and page counts are as
  // expected.
  allocator.lock().set_can_alloc(true);
  v.reserve(RSV_COUNT);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  // Freeze the allocator again and attempt to reserve an additional RSV_COUNT
  // elements. Verify the length, capacity, and page counts are unchanged.
  allocator.lock().set_can_alloc(false);
  v.reserve(RSV_COUNT);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  let (_, exp_pages, exp_capacity) =
    calc_pages_and_capacity(RSV_COUNT * 2, size_of::<TestStruct>());

  // Unfreeze the allocator and allow it to reserve the additional elements.
  // Verify it is able to reserve the extra capacity.
  allocator.lock().set_can_alloc(true);
  v.reserve(RSV_COUNT * 2);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);
}

/// Verify a vector can be resized in both directions.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_resize(context: &mut test::TestContext) {
  const MAGIC: u8 = 42;
  const RSV_COUNT: usize = 10;
  const RSV_COUNT2: usize = 5;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let free_mem = allocator.lock().get_free_mem();
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(RSV_COUNT, size_of::<TestStruct>());
  let page_size = arch::get_page_size();
  let obj_mem_size = RSV_COUNT * size_of::<TestStruct>();

  // Resize the vector. The vector should allocate at least RSV_COUNT elements.
  v.resize(RSV_COUNT, TestStruct::new(MAGIC));
  check_eq!(context, v.length, RSV_COUNT);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);
  check_eq!(context, allocator.lock().get_free_mem(), free_mem - exp_pages * page_size);

  let ptr = v.ptr.as_ptr() as *const u8;
  check!(context, verify_memory(ptr, 0, obj_mem_size, MAGIC));

  let old_capacity = v.capacity;
  let old_pages = v.pages;
  let obj_mem_size = RSV_COUNT2 * size_of::<TestStruct>();
  let rem_obj_size = RSV_COUNT * size_of::<TestStruct>() - obj_mem_size;

  // Resize the vector to a smaller length. The capacity and allocated memory
  // should not change. The destructors of the dropped elements should have
  // overwritten the objects with the poison byte to indicate they were dropped.
  v.resize(RSV_COUNT2, TestStruct::new(MAGIC));
  check_eq!(context, v.length, RSV_COUNT2);
  check_eq!(context, v.capacity, old_capacity);
  check_eq!(context, v.pages, old_pages);
  {
    let lock = allocator.lock();
    check_eq!(context, lock.get_alloc_count(), 1);
    check_eq!(context, lock.get_alloc_total(), exp_pages);
    check_eq!(context, lock.get_free_count(), 1);
    check_eq!(context, lock.get_free_total(), 0);
  }

  check!(context, verify_memory(ptr, 0, obj_mem_size, MAGIC));
  check!(context, verify_memory(ptr, obj_mem_size, rem_obj_size, bits::POISON_BYTE));
}

/// Verify that the vector is unmodified if allocation fails during a resize.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_resize_alloc_fail(context: &mut test::TestContext) {
  const MAGIC: u8 = 42;
  const RSV_COUNT: usize = 20;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(RSV_COUNT, size_of::<TestStruct>());

  // Freeze the allocator and verify resize does nothing.
  allocator.lock().set_can_alloc(false);
  v.resize(RSV_COUNT, TestStruct::new(MAGIC));
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, 0);
  check_eq!(context, v.pages, 0);

  // Unfreeze the allocator and verify resize expands the vector.
  allocator.lock().set_can_alloc(true);
  v.resize(RSV_COUNT, TestStruct::new(MAGIC));
  check_eq!(context, v.length, RSV_COUNT);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);
}

/// Verify a vector can be truncated to zero and free its memory.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_truncate_to_zero(context: &mut test::TestContext) {
  const MAGIC: u8 = 42;
  const RSV_COUNT: usize = 10;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(RSV_COUNT, size_of::<TestStruct>());

  v.resize(RSV_COUNT, TestStruct::new(MAGIC));
  check_eq!(context, v.length, RSV_COUNT);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  v.truncate(0);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, 0);
  check_eq!(context, v.pages, 0);
  {
    let lock = allocator.lock();
    check_eq!(context, lock.get_free_count(), 2);
    check_eq!(context, lock.get_free_total(), exp_pages);
  }
}

/// Verify the vector can shrink to fit the current number of elements.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_shrink_to_fit(context: &mut test::TestContext) {
  const MAGIC: u8 = 42;
  const RSV_COUNT: usize = 10;
  const RSV_COUNT2: usize = 5;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(RSV_COUNT, size_of::<TestStruct>());

  // Reserve space for RSV_COUNT without adding any elements.
  v.reserve(RSV_COUNT);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  // Resize the vector by adding RSV_COUNT2 elements.
  v.resize(RSV_COUNT2, TestStruct::new(MAGIC));
  check_eq!(context, v.length, RSV_COUNT2);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  let ptr = v.ptr.as_ptr() as *const u8;
  let obj_mem_size = RSV_COUNT2 * size_of::<TestStruct>();
  check!(context, verify_memory(ptr, 0, obj_mem_size, MAGIC));

  let (_, exp_pages2, exp_capacity2) = calc_pages_and_capacity(RSV_COUNT2, size_of::<TestStruct>());

  // Shrink to fit and verify the vector is reallocated.
  v.shrink_to_fit();
  check_eq!(context, v.length, RSV_COUNT2);
  check_eq!(context, v.capacity, exp_capacity2);
  check_eq!(context, v.pages, exp_pages2);
  {
    let lock = allocator.lock();
    check_eq!(context, lock.get_alloc_count(), 2);
    check_eq!(context, lock.get_alloc_total(), exp_pages + exp_pages2);
    check_eq!(context, lock.get_free_count(), 2);
    check_eq!(context, lock.get_free_total(), exp_pages);
  }
}

/// Verify elements can be pushed to and popped from the end of the vector.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_push_pop(context: &mut test::TestContext) {
  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Calculate a number of elements to push that forces reallocations.
  let page_size = arch::get_page_size();
  let push_count = (page_size * 3) / size_of::<TestStruct>();
  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(push_count, size_of::<TestStruct>());

  // Push elements to the vector, then verify their contents.
  for i in 0..push_count {
    check!(context, v.push(TestStruct::new((i + 1) as u8)));
  }

  check_eq!(context, v.length, push_count);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  for (i, element) in v.iter().enumerate() {
    check!(
      context,
      verify_memory(element as *const _ as *const u8, 0, size_of::<TestStruct>(), (i + 1) as u8)
    );
  }

  // Pop each element and verify the contents. The vector memory is not
  // overwritten since the object is moved to the local element variable.
  for i in 0..push_count {
    let element = v.pop().unwrap();
    check_eq!(context, v.length, push_count - i - 1);
    check!(
      context,
      verify_memory(
        &element as *const _ as *const u8,
        0,
        size_of::<TestStruct>(),
        (push_count - i) as u8
      )
    );
  }
}

/// Verify the vector is unmodified if allocation fails during a push.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_push_alloc_fail(context: &mut test::TestContext) {
  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(1, size_of::<TestStruct>());

  // Freeze the allocator and attempt to push.
  allocator.lock().set_can_alloc(false);
  check_eq!(context, v.push(TestStruct::new(1)), false);
  check_eq!(context, v.length, 0);
  check_eq!(context, v.capacity, 0);
  check_eq!(context, v.pages, 0);

  // Unfreeze the allocator and attempt to push.
  allocator.lock().set_can_alloc(true);
  check_eq!(context, v.push(TestStruct::new(1)), true);
  check_eq!(context, v.length, 1);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);
}

/// Verify elements can be inserted into the vector.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert(context: &mut test::TestContext) {
  const RSV_COUNT: usize = 10;
  const INSERT_AT: usize = 5;
  const MAGIC: u8 = 42;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Verify insert with empty vector fails.
  check_eq!(context, v.insert(0, TestStruct::new(MAGIC)), false);

  // Push elements to the vector.
  for i in 0..RSV_COUNT {
    _ = v.push(TestStruct::new((i + 1) as u8));
  }

  check_eq!(context, v.length, RSV_COUNT);

  // Verify out-of-bounds insert fails.
  check_eq!(context, v.insert(RSV_COUNT + 1, TestStruct::new(MAGIC)), false);

  // Insert a new element to the middle of the vector and verify the state.
  check!(context, v.insert(INSERT_AT, TestStruct::new(MAGIC)));
  check_eq!(context, v.length, RSV_COUNT + 1);

  for (i, element) in v.iter().enumerate() {
    let exp_fill = if i < INSERT_AT {
      (i + 1) as u8
    } else if i == INSERT_AT {
      MAGIC
    } else {
      i as u8
    };

    check!(
      context,
      verify_memory(element as *const _ as *const u8, 0, size_of::<TestStruct>(), exp_fill)
    );
  }
}

/// Verify the vector is unmodified if allocation fails during an insert.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_insert_alloc_fail(context: &mut test::TestContext) {
  const MAGIC: u8 = 42;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Calculate a number of elements to insert that will require a reallocation
  // on insert.
  let page_size = arch::get_page_size();
  let push_count = page_size / size_of::<TestStruct>();
  let insert_at = push_count / 2;
  let (_, exp_pages, exp_capacity) = calc_pages_and_capacity(push_count, size_of::<TestStruct>());

  // Push elements to the vector.
  for i in 0..push_count {
    _ = v.push(TestStruct::new((i + 1) as u8));
  }

  // Freeze the allocator and attempt to insert.
  allocator.lock().set_can_alloc(false);
  check_eq!(context, v.insert(insert_at, TestStruct::new(MAGIC)), false);
  check_eq!(context, v.length, push_count);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);

  let (_, exp_pages, exp_capacity) =
    calc_pages_and_capacity(push_count + 1, size_of::<TestStruct>());

  // Unfreeze the allocator and attempt to insert.
  allocator.lock().set_can_alloc(true);
  check!(context, v.insert(insert_at, TestStruct::new(MAGIC)));
  check_eq!(context, v.length, push_count + 1);
  check_eq!(context, v.capacity, exp_capacity);
  check_eq!(context, v.pages, exp_pages);
}

/// Verify elements can be removed from the vector.
///
/// # Parameters
///
/// * `context` - The test context.
fn test_remove(context: &mut test::TestContext) {
  const RSV_COUNT: usize = 10;
  const REMOVE_AT: usize = 5;

  reset_test_memory();

  let allocator = SpinLock::new(make_test_page_allocator());
  let mut v = Vector::<TestPageAllocator, TestStruct>::new(&allocator);

  // Verify remove with empty vector fails.
  check_none!(context, v.remove(0));

  // Push elements to the vector.
  for i in 0..RSV_COUNT {
    _ = v.push(TestStruct::new((i + 1) as u8));
  }

  check_eq!(context, v.length, RSV_COUNT);

  // Verify out-of-bounds remove fails.
  check_none!(context, v.remove(RSV_COUNT));

  // Remove an element from the middle of the vector and verify the state.
  let element = v.remove(REMOVE_AT);
  check_not_none!(context, element);

  let element = element.unwrap();
  check!(
    context,
    verify_memory(
      &element as *const _ as *const u8,
      0,
      size_of::<TestStruct>(),
      (REMOVE_AT + 1) as u8
    )
  );
  check_eq!(context, v.length, RSV_COUNT - 1);

  for (i, element) in v.iter().enumerate() {
    let exp_fill = if i < REMOVE_AT {
      (i + 1) as u8
    } else {
      (i + 2) as u8
    };

    check!(
      context,
      verify_memory(element as *const _ as *const u8, 0, size_of::<TestStruct>(), exp_fill)
    );
  }
}
