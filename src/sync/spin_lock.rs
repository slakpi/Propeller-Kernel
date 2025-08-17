//! Spin Lock Primitive

use crate::arch::sync::{spin_lock, spin_try_lock, spin_unlock};
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut, Drop};
use core::ptr;

/// Guard object for lock ownership. A SpinLock constructs a guard object when
/// a thread acquires the lock. A thread releases the lock by dropping the guard
/// object.
pub struct SpinLockGuard<'lock, T> {
  lock: &'lock SpinLock<T>,
}

impl<'lock, T> SpinLockGuard<'lock, T> {
  /// Construct a guard object after acquiring a lock.
  ///
  /// # Parameters
  ///
  /// * `lock` - The acquired lock.
  pub fn new(lock: &'lock SpinLock<T>) -> Self {
    SpinLockGuard { lock }
  }
}

impl<T> Drop for SpinLockGuard<'_, T> {
  /// Unlock on drop.
  fn drop(&mut self) {
    spin_unlock(ptr::addr_of!(self.lock.lock_var) as usize);
  }
}

impl<T> Deref for SpinLockGuard<'_, T> {
  type Target = T;

  /// Obtain a reference to the protected object.
  fn deref(&self) -> &Self::Target {
    unsafe { &*self.lock.obj.get() }
  }
}

impl<T> DerefMut for SpinLockGuard<'_, T> {
  /// Obtain a mutable reference to the protected object.
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { &mut *self.lock.obj.get() }
  }
}

/// A spin lock protects a wrapped object. A guard object must be obtained
/// using the lock method to access the protected object.
pub struct SpinLock<T> {
  /// The protected object. UnsafeCell is used to allow interior mutability.
  obj: UnsafeCell<T>,

  /// The lock variable. The spin lock spins on the address of this variable and
  /// uses its value as an indicator of lock status.
  lock_var: u32,
}

impl<T> SpinLock<T> {
  /// Construct a new spin lock to protect the specified object.
  pub const fn new(obj: T) -> Self {
    SpinLock {
      obj: UnsafeCell::new(obj),
      lock_var: 0,
    }
  }

  /// Block to acquire the spin lock.
  ///
  /// # Returns
  ///
  /// A guard object upon acquiring the lock.
  pub fn lock(&self) -> SpinLockGuard<'_, T> {
    spin_lock(ptr::addr_of!(self.lock_var) as usize);
    SpinLockGuard::new(self)
  }

  /// Attempt to acquire the lock without blocking.
  ///
  /// # Returns
  ///
  /// A guard object upon acquiring the lock, or None if the lock is already
  /// acquired by another thread.
  pub fn try_lock(&self) -> Option<SpinLockGuard<'_, T>> {
    if !spin_try_lock(ptr::addr_of!(self.lock_var) as usize) {
      return None;
    }

    Some(SpinLockGuard::new(self))
  }
}
