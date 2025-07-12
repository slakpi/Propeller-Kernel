/// AArch64 exception trap.
///
/// # Parameters
///
/// * `esr_el1` - Exception Syndrome Register value.
/// * `far_el1` - Fault Address Register value.
/// * `cpu_context` - Pointer to the saved CPU context structure.
#[unsafe(no_mangle)]
extern "C" fn trap_exception(esr_el1: usize, far_el1: usize, cpu_context: usize) {
}
