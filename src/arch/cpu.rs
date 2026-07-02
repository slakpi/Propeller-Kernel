//! Architecture-Dependent CPU Utilities

unsafe extern "C" {
  fn cpu_get_id() -> usize;
  fn cpu_halt() -> !;
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
