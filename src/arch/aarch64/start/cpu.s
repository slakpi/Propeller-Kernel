//! AArch64 Low-Level CPU Utilities

.equ CPU_AFFINITY_MASK, 0x000000ff00ffffff

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
  wfi                       // Use a wait to keep this from being a busy loop.
  b       1b                // Infinite loop.


///-----------------------------------------------------------------------------
///
/// Get the current core ID.
.global cpu_get_id
cpu_get_id:
  mrs     x0, mpidr_el1
  ldr     x1, =CPU_AFFINITY_MASK
  and     x0, x0, x1
  ret
