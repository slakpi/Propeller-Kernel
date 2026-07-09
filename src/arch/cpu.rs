//! Architecture-Dependent CPU Utilities

unsafe extern "C" {
  fn cpu_flush_data_cache_by_va(virt_addr: usize);
  fn cpu_get_id() -> usize;
  fn cpu_halt() -> !;
  fn cpu_send_event();
}

/// Clean and invalidate the data cache by virtual address.
///
/// # Parameters
///
/// * `virt_addr` - The virtual address to invalidate.
pub fn flush_data_cache_by_va(virt_addr: usize) {
  unsafe { cpu_flush_data_cache_by_va(virt_addr) };
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

/// Signal all cores.
pub fn send_event() {
  unsafe { cpu_send_event() }
}
