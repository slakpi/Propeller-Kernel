use crate::arch;

/// AArch64 exception handler.
///
/// # Parameters
///
/// * `esr_el1` - Exception Syndrome Register value.
/// * `far_el1` - Fault Address Register value.
/// * `cpu_context` - Pointer to the saved CPU context structure.
#[unsafe(no_mangle)]
extern "C" fn pk_handle_exception(esr_el1: usize, far_el1: usize, cpu_context: usize) {
  arch::common::cpu::halt();
}
