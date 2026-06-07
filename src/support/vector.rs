//! Vector Utilities
//!
//! The Vector object is based on std::vec::Vec with some minor variations in
//! the interface to account for the expected restriction of the amount of
//! contiguous memory the vector can allocate.
//!
//!   NOTE: Vector is intended for managing kernel data. The allocator used
//!         to allocate vectors MUST allocate from linear memory.
//!
//!   NOTE: A Vector will always allocate at least one page.
//!
//!   NOTE: Vectors must be contiguous memory. As such, they can never be larger
//!         than the largest block allowed by the provided allocator.

#[cfg(feature = "module_tests")]
mod tests;

use crate::arch;
use crate::arch::memory::PageAllocator;
use crate::sync::spin_lock::SpinLock;
use core::ops::{Index, IndexMut};
use core::ptr::{self, NonNull};
use core::slice::{Iter, IterMut};

/// A simple vector based on std::vec::Vec.
struct Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  allocator: &'alloc SpinLock<A>,
  capacity: usize,
  length: usize,
  pages: usize,
  ptr: NonNull<T>,
}

impl<'alloc, A, T> Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  /// Construct a new vector with the provided allocator.
  ///
  /// # Parameters
  ///
  /// * `allocator` - The allocator to use.
  pub const fn new(allocator: &'alloc SpinLock<A>) -> Self {
    Self {
      allocator,
      capacity: 0,
      length: 0,
      pages: 0,
      ptr: NonNull::dangling(),
    }
  }

  /// Get the number of objects the vector hold.
  pub const fn capacity(&self) -> usize {
    self.capacity
  }

  /// Get the number of objects the vector currently holds.
  pub const fn len(&self) -> usize {
    self.length
  }

  /// Determine if the vector contains any objects.
  pub const fn is_empty(&self) -> bool {
    self.length == 0
  }

  /// Get a raw pointer to the vector data.
  pub const fn as_ptr(&self) -> *const T {
    self.ptr.as_ptr()
  }

  /// Get a mutable raw pointer to the vector data.
  pub const fn as_mut_ptr(&mut self) -> *mut T {
    self.ptr.as_ptr()
  }

  /// Get the vector as a slice.
  pub const fn as_slice(&self) -> &[T] {
    unsafe { &*ptr::slice_from_raw_parts(self.ptr.as_ptr(), self.length) }
  }

  /// Get the vector as a mutable slice.
  pub const fn as_mut_slice(&mut self) -> &mut [T] {
    unsafe { &mut *ptr::slice_from_raw_parts_mut(self.ptr.as_ptr(), self.length) }
  }

  /// Get a vector iterator.
  pub fn iter(&self) -> Iter<'_, T> {
    self.as_slice().iter()
  }

  /// Get a mutable vector iterator.
  pub fn iter_mut(&mut self) -> IterMut<'_, T> {
    self.as_mut_slice().iter_mut()
  }

  /// Insert an element into the vector at a given index.
  ///
  /// # Parameters
  ///
  /// * `index` - The insertion index.
  /// * `element` - The element to insert.
  ///
  /// # Description
  ///
  /// Attempts to reserve space for one additional element, then shifts the
  /// current elements as necessary before moving the new element into the
  /// vector.
  ///
  /// Insert will panic if the new index is greater than or equal to the current
  /// length, or if a reallocation is necessary and new memory could not be
  /// allocated.
  ///
  /// # Returns
  ///
  /// True if able to insert the element, false otherwise.
  pub fn insert(&mut self, index: usize, element: T) -> bool {
    if index >= self.length {
      return false;
    }

    self.reserve(1);

    if self.capacity < self.length + 1 {
      return false;
    }

    unsafe {
      let ptr = self.as_mut_ptr();
      ptr::copy(ptr.add(index), ptr.add(index + 1), self.length - index);
      ptr::write(ptr.add(index), element);
    }

    self.length += 1;

    true
  }

  /// Remove the element at a given index.
  ///
  /// # Parameters
  ///
  /// * `index` - The removal index.
  ///
  /// # Description
  ///
  /// Removes the element at the specified index. The capacity of the vector
  /// will not change.
  ///
  /// Remove will panic if the index is greater than or equal to the current
  /// length.
  ///
  /// # Returns
  ///
  /// The element previously at the given index, or None if the index is out of
  /// range.
  pub fn remove(&mut self, index: usize) -> Option<T> {
    if index >= self.length {
      return None;
    }

    let element;

    unsafe {
      let ptr = self.as_mut_ptr();
      element = ptr::read(ptr.add(index));
      ptr::copy(ptr.add(index + 1), ptr.add(index), self.length - index - 1);
    }

    self.length -= 1;

    Some(element)
  }

  /// Push an element to the end of the vector.
  ///
  /// # Parameters
  ///
  /// * `element` - The element to push.
  ///
  /// # Description
  ///
  /// Push will attempt to reserve capacity for the current length plus one. If
  /// memory allocation fails, push will panic.
  ///
  /// # Returns
  ///
  /// True if able to push the element, false otherwise.
  pub fn push(&mut self, element: T) -> bool {
    self.reserve(1);

    if self.capacity < self.length + 1 {
      return false;
    }

    unsafe {
      ptr::write(self.as_mut_ptr().add(self.length), element);
    }

    self.length += 1;

    true
  }

  /// Pop an element from the end of the vector.
  ///
  /// # Returns
  ///
  /// The element previously at the end of the vector, or None if the vector is
  /// empty.
  pub fn pop(&mut self) -> Option<T> {
    if self.length == 0 {
      return None;
    }

    let element = unsafe { ptr::read(self.as_mut_ptr().add(self.length - 1)) };
    self.length -= 1;
    Some(element)
  }

  /// Reserves capacity for a new minimum number of elements.
  ///
  /// # Parameters
  ///
  /// * `additional` - The number of elements to add to the current capacity.
  ///
  /// # Description
  ///
  /// Allocates memory as necessary to increase the vector's capacity to at
  /// least the current length plus the additional number of elements. The
  /// additional elements are left uninitialized and the length does not change.
  ///
  /// If the vector already has sufficient capacity, no action is taken.
  ///
  /// If additional memory cannot be allocated, the vector is not modified.
  pub fn reserve(&mut self, additional: usize) {
    if self.length + additional <= self.capacity {
      return;
    }

    let page_size = arch::get_page_size();
    let req_size = (self.length + additional) * size_of::<T>();
    let req_pages = (req_size + page_size - 1) / page_size;

    self.reallocate(req_pages);
  }

  /// Truncate the length of the vector by dropping excess elements.
  ///
  /// # Parameters
  ///
  /// * `len` - The new length of the vector.
  ///
  /// # Description
  ///
  /// If the new length is greater than or equal to the current length, no
  /// action is taken.
  ///
  /// Otherwise, all elements in the range [len, current len) are dropped and
  /// the length is updated to the new length. The vector will retain its
  /// capacity unless the new length is 0.
  pub fn truncate(&mut self, len: usize) {
    if len > self.length {
      return;
    }

    unsafe {
      let ptr = self.ptr.as_ptr();
      for i in len..self.length {
        ptr::drop_in_place(ptr.add(i));
      }
    }

    if len == 0 {
      self.free_mem();
      return;
    }

    self.length = len;
  }

  /// Shrinks the capacity of the vector as much as possible.
  ///
  /// # Description
  ///
  /// If unable to allocate a smaller block of memory, the vector is left
  /// unmodified.
  pub fn shrink_to_fit(&mut self) {
    let page_size = arch::get_page_size();
    let req_pages = (self.length * size_of::<T>() + page_size - 1) / page_size;

    if req_pages >= self.pages {
      return;
    }

    self.reallocate(req_pages);
  }

  /// Reallocate the vector into a smaller or larger memory block.
  ///
  /// # Parameters
  ///
  /// * `req_pages` - The required number of pages.
  ///
  /// # Description
  ///
  /// While the new required number of pages may be smaller than the current
  /// number of pages, it must be at least large enough to hold the current
  /// number of elements.
  ///
  /// If the required number of pages is too small for the current contents of
  /// the vector or the vector is unable to allocate a new block of memory, the
  /// vector is not modified.
  fn reallocate(&mut self, req_pages: usize) {
    let virt_base = arch::get_kernel_virtual_base();
    let page_size = arch::get_page_size();

    if self.length * size_of::<T>() >= req_pages * page_size {
      return;
    }

    let Some((phys_addr, pages)) = self.allocator.lock().alloc(req_pages) else {
      return;
    };

    // Copy the vector to the new block.
    let size = pages * page_size;
    let capacity = size / size_of::<T>();
    let ptr = (phys_addr + virt_base) as *mut T;
    unsafe { ptr::copy(self.ptr.as_ptr(), ptr, self.length) };

    let len = self.length;
    self.free_mem();
    self.ptr = NonNull::new(ptr).unwrap();
    self.capacity = capacity;
    self.pages = pages;
    self.length = len;
  }

  /// Free the memory backing the vector.
  ///
  /// # Description
  ///
  /// The pointer is only valid if the number of pages is greater than zero.
  /// Since PageAllocator implementations rely on physical addresses, the page
  /// count is checked before converting the virtual address to physical.
  /// Freeing a null pointer is safe.
  fn free_mem(&mut self) {
    let phys_addr = if self.pages > 0 {
      self.ptr.as_ptr() as usize - arch::get_kernel_virtual_base()
    } else {
      0
    };

    self.allocator.lock().free(phys_addr, self.pages);
    self.capacity = 0;
    self.pages = 0;
    self.length = 0;
  }
}

impl<'alloc, A, T> Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized + Clone,
{
  /// Resize the vector filling any new elements with the specified value.
  ///
  /// # Parameters
  ///
  /// * `new_len` - The new vector length.
  /// * `value` - The value to copy into new elements.
  ///
  /// # Description
  ///
  /// If the new length is less than the current length, the vector is
  /// truncated. If the new length is greater than or equal to the current
  /// length, but less than the capacity, then no reallocation is necessary and
  /// the empty elements are filled in up to the new length.
  ///
  /// If reallocation is necessary and fails, no action is taken. Otherwise, the
  /// vector is copied to the new memory and the empty elements are filled up to
  /// the new length.
  pub fn resize(&mut self, new_len: usize, value: T) {
    if new_len < self.length {
      self.truncate(new_len);
      return;
    }

    if new_len == self.length {
      return;
    }

    self.reserve(new_len - self.length);

    // Verify reserve was able to allocate memory.
    if self.capacity < new_len {
      return;
    }

    for i in self.length..new_len {
      unsafe {
        ptr::write(self.ptr.as_ptr().add(i), value.clone());
      }
    }

    self.length = new_len;
  }
}

impl<'alloc, A, T> Drop for Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  /// See `Drop::drop()`.
  fn drop(&mut self) {
    self.truncate(0)
  }
}

impl<'alloc, A, T> Index<usize> for Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  type Output = T;

  /// See `Index::index()`.
  fn index(&self, index: usize) -> &Self::Output {
    Index::index(self.as_slice(), index)
  }
}

impl<'alloc, A, T> IndexMut<usize> for Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  /// See `IndexMut::index_mut()`.
  fn index_mut(&mut self, index: usize) -> &mut Self::Output {
    IndexMut::index_mut(self.as_mut_slice(), index)
  }
}

impl<'vec, 'alloc, A, T> IntoIterator for &'vec Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  type Item = &'vec T;
  type IntoIter = Iter<'vec, T>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter()
  }
}

impl<'vec, 'alloc, A, T> IntoIterator for &'vec mut Vector<'alloc, A, T>
where
  A: PageAllocator,
  T: Sized,
{
  type Item = &'vec mut T;
  type IntoIter = IterMut<'vec, T>;

  fn into_iter(self) -> Self::IntoIter {
    self.iter_mut()
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut crate::test::TestContext) {
  tests::run_tests(context);
}
