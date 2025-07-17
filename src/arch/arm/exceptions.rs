//! ARM Exception Handling

use crate::arch;

/// ARM exception handler.
///
/// # Parameters
/// 
/// * `exception` - The exception type.
/// * `cpu_context` - Pointer to the saved CPU context structure.
#[unsafe(no_mangle)]
extern "C" fn pk_handle_exception(_exception: usize, _cpu_context: usize) {
  arch::common::cpu::halt();
}
