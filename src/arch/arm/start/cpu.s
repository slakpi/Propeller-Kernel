//! ARM Low-Level CPU Utilities

///-----------------------------------------------------------------------------
///
/// Halt the caller.
///
/// # Description
///
/// Halts the current core using an infinite wait loop.
.global cpu_halt
cpu_halt:
1:
  wfi                       // Wait for interrupt.
  b       1b                // Infinite loop.


///-----------------------------------------------------------------------------
///
/// Get the current core ID.
.global cpu_get_id
cpu_get_id:
  mrc     p15, 0, r0, c0, c0, 5
  and     r0, r0, #0xff
  mov     pc, lr
