//! ARM Common CPU Utilities

pub use crate::arch::common::cpu::*;

unsafe extern "C" {
  fn cpu_halt() -> !;
  fn cpu_get_id() -> usize;
}

/// Halt the caller.
pub fn halt() -> ! {
  unsafe { cpu_halt() };
}

/// Get the current core ID.
pub fn get_id() -> usize {
  unsafe { cpu_get_id() }
}
