//! ARM Low-Level Task Management

///-----------------------------------------------------------------------------
///
/// Get the current task address from the TPIDRURO register. See B3.17.
.global task_get_current_task_addr
task_get_current_task_addr:
  mrc     p15, 0, r0, c13, c0, 3
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Set the current task address. See B3.17.
///
/// # Parameters
///
/// r0 - The task address.
.global task_set_current_task_addr
task_set_current_task_addr:
  mcr     p15, 0, r0, c13, c0, 3
  mov     pc, lr
