//! AArch64 Exception Handling

use crate::arch;

/// Exception handler.
///
/// # Parameters
///
/// * `esr_el1` - Exception Syndrome Register value.
/// * `far_el1` - Fault Address Register value.
/// * `cpu_context` - Pointer to the saved CPU context structure.
#[unsafe(no_mangle)]
extern "C" fn pk_handle_exception(_esr_el1: usize, _far_el1: usize, _cpu_context: usize) {
  arch::cpu::halt();
}
