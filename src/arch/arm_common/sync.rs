//! ARM Common Synchronization Primitives

unsafe extern "C" {
  fn sync_spin_lock(lock_addr: usize);
  fn sync_spin_try_lock(lock_addr: usize) -> u32;
  fn sync_spin_unlock(lock_addr: usize);
}

/// Spin lock.
///
/// # Parameters
///
/// * `lock_addr` - The address of a 32-bit lock value.
pub fn spin_lock(lock_addr: usize) {
  unsafe { sync_spin_lock(lock_addr) };
}

/// Attempt to obtain a spin lock.
///
/// # Parameters
///
/// * `lock_addr` - The address of a 32-bit lock value.
///
/// # Returns
///
/// True if the lock succeeded, false otherwise.
pub fn spin_try_lock(lock_addr: usize) -> bool {
  unsafe { sync_spin_try_lock(lock_addr) == 0 }
}

/// Release a spin lock.
///
/// # Parameters
///
/// * `lock_addr` - The address of a 32-bit lock value.
///
/// # Description
///
///   NOTE: The caller must ensure it has obtained the lock.
pub fn spin_unlock(lock_addr: usize) {
  unsafe { sync_spin_unlock(lock_addr) };
}
