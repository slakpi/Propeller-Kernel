//! Architecture-Dependent CPU Utilities

/// Common contract functions.
unsafe extern "C" {
  fn cpu_data_sync_barrier();
  fn cpu_flush_data_cache_by_va(virt_addr: usize);
  fn cpu_flush_tlb_by_va(virt_addr: usize, asid: usize);
  fn cpu_flush_tlb_by_va_range(virt_addr: usize, pages: usize, asid: usize);
  fn cpu_get_id() -> usize;
  fn cpu_halt() -> !;
  fn cpu_instruction_sync_barrier();
  fn cpu_send_event();
  fn cpu_sync_barriers();
}

/// ARM contract functions.
#[cfg(target_arch = "arm")]
unsafe extern "C" {
  fn cpu_flush_bp_by_va(virt_addr: usize, asid: usize);
  fn cpu_flush_bp_by_va_range(virt_addr: usize, pages: usize, asid: usize);
}

/// Issue a data synchronization barrier.
pub fn data_sync_barrier() {
  unsafe {
    cpu_data_sync_barrier();
  }
}

/// Clean and invalidate the branch predictors by virtual address.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
/// * `asid` - The address space ID to invalidate.
#[cfg(target_arch = "arm")]
pub fn flush_bp_by_va(virt_addr: usize, asid: usize) {
  unsafe { cpu_flush_bp_by_va(virt_addr, asid) };
}

/// Clean and invalidate the branch predictors by virtual address range.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
/// * `pages` - The number of pages to invalidate.
/// * `asid` - The address space ID to invalidate.
#[cfg(target_arch = "arm")]
pub fn flush_bp_by_va_range(virt_addr: usize, pages: usize, asid: usize) {
  unsafe { cpu_flush_bp_by_va_range(virt_addr, pages, asid) };
}

/// Clean and invalidate the data cache by virtual address.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
pub fn flush_data_cache_by_va(virt_addr: usize) {
  unsafe { cpu_flush_data_cache_by_va(virt_addr) };
}

/// Clean and invalidate the TLB by virtual address.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
/// * `asid` - The address space ID to invalidate.
pub fn flush_tlb_by_va(virt_addr: usize, asid: usize) {
  unsafe { cpu_flush_tlb_by_va(virt_addr, asid) };
}

/// Clean and invalidate the TLB by virtual address range.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
/// * `pages` - The number of pages to invalidate.
/// * `asid` - The address space ID to invalidate.
pub fn flush_tlb_by_va_range(virt_addr: usize, pages: usize, asid: usize) {
  unsafe { cpu_flush_tlb_by_va_range(virt_addr, pages, asid) };
}

/// Get the current core ID.
pub fn get_id() -> usize {
  unsafe { cpu_get_id() }
}

/// Halt the caller.
///
/// # Description
///
/// Halts the current core using an infinite wait loop.
pub fn halt() -> ! {
  unsafe { cpu_halt() }
}

/// Issue a data synchronization barrier.
pub fn instruction_sync_barrier() {
  unsafe {
    cpu_instruction_sync_barrier();
  }
}

/// Signal all cores.
pub fn send_event() {
  unsafe { cpu_send_event() }
}

/// Issue synchronization barriers.
pub fn sync_barriers() {
  unsafe {
    cpu_sync_barriers();
  }
}
