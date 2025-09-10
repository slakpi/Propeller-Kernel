//! AArch64 Low-Level Task Management

///-----------------------------------------------------------------------------
///
/// Get the current task address from the TPIDR_EL1 register. See D17.2.140.
.global task_get_current_task_addr
task_get_current_task_addr:
  mrs     x0, tpidr_el1
  ret


///-----------------------------------------------------------------------------
///
/// Set the current task address. See D17.2.140.
///
/// # Parameters
///
/// x0 - The task address.
.global task_set_current_task_addr
task_set_current_task_addr:
  msr     tpidr_el1, x0
  ret
