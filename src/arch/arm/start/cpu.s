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


///-----------------------------------------------------------------------------
///
/// Clean and invalidate the data cache by virtual address.
///
/// # Parameters
///
/// * r0 - The virtual address to invalidate.
.global cpu_flush_data_cache_by_va
cpu_flush_data_cache_by_va:
  mcr     p15, 0, r0, c7, c14, 1
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Signal all cores.
.global cpu_send_event
cpu_send_event:
  sev
  mov     pc, lr
