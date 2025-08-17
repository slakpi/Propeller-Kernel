//! AArch64 Spin Lock

/*----------------------------------------------------------------------------*/
/// Acquire a spin lock. See K13.3.4.
///
/// # Parameters
///
/// * x0 - The lock memory location.
.global sync_spin_lock
sync_spin_lock:
  mov     w10, #1

  sevl                      // Prevent waiting at WFE in the first iteration.

// Tell the memory system to prefetch for store into the L1 cache. This can
// speed up stores to the address, but really depends on the physical processor
// implementation.
  prfm    pstl1keep, [x0]

1:
  wfe                       // Sleep
  ldaxr   w9, [x0]          // Load the lock value.
  cbnz    w9, 1b            // If w9 != 0, the lock is already acquired.
  stxr    w9, w10, [x0]     // Attempt to write w10.
  cbnz    w9, 1b            // If w9 != 0, writing failed.

  ret                       // Lock acquired.


/*----------------------------------------------------------------------------*/
/// Attempt to acquire a spin lock. See K13.3.4.
///
/// # Parameters
///
/// * x0 - The lock memory location.
///
/// # Returns
///
/// 0 if able to acquire the lock, non-zero otherwise.
.global sync_spin_try_lock
sync_spin_try_lock:
  mov     x9, x0
  mov     w10, #1
  prfm    pstl1keep, [x0]   // See sync_spin_lock.
  ldaxr   w0, [x9]          // Load the lock value.
  cbnz    w0, 1f            // If w0 != 0, return with a non-zero value.
  stxr    w0, w10, [x9]     // Attempt to write w10, w0 is 0 if successful.

1:
  ret


/*----------------------------------------------------------------------------*/
/// Release a spin lock. See K13.3.2.
///
/// # Parameters
///
/// * x0 - The lock memory location.
///
/// # Description
///
///   NOTE: The caller must ensure it has acquired the lock.
.global sync_spin_unlock
sync_spin_unlock:
  stlr    wzr, [x0]         // Release the lock.
  ret
