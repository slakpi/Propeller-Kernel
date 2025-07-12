//! AArch64 Low-Level CPU Utilities

///-----------------------------------------------------------------------------
///
/// Halt the caller.
///
/// # Description
///
/// Halts the current core using an infinite wait loop.
.global cpu_halt
cpu_halt:
  brk     #0                // Trigger hardware breakpoint.
1:
  wfi                       // Use a wait to keep this from being a busy loop.
  b       1b                // Infinite loop.


///-----------------------------------------------------------------------------
///
/// Get the current core ID.
.global cpu_get_id
cpu_get_id:
  mrs     x0, mpidr_el1
  and     x0, x0, #0xff
  ret
