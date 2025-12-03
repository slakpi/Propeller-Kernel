//! ARM Low-Level CPU Utilities

.equ CPU_AFFINITY_MASK, 0x00ffffff

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
  ldr     r1, =CPU_AFFINITY_MASK
  and     r0, r0, r1
  mov     pc, lr
