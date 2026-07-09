//! AArch64 Low-Level CPU Utilities

.equ CPU_AFFINITY_MASK, 0x000000ff00ffffff

///-----------------------------------------------------------------------------
///
/// Halt the caller.
///
/// # Description
///
/// Halts the current core using an infinite wait loop.
///
///   NOTE: This function will be called by the secondary cores before they have
///         stacks. This function MUST not modify callee-saved registers or call
///         other functions.
.global cpu_halt
cpu_halt:
1:
  wfi                       // Use a wait to keep this from being a busy loop.
  b       1b                // Infinite loop.


///-----------------------------------------------------------------------------
///
/// Get the current core ID.
///
/// # Description
///
///   NOTE: This function will be called by the secondary cores before they have
///         stacks. This function MUST not modify callee-saved registers or call
///         other functions.
.global cpu_get_id
cpu_get_id:
  mrs     x0, mpidr_el1
  ldr     x1, =CPU_AFFINITY_MASK
  and     x0, x0, x1
  ret


///-----------------------------------------------------------------------------
///
/// Clean and invalidate the data cache by virtual address.
///
/// # Parameters
///
/// * x0 - The virtual address to invalidate.
.global cpu_flush_data_cache_by_va
cpu_flush_data_cache_by_va:
  dc      civac, x0
  dsb     sy
  ret


///-----------------------------------------------------------------------------
///
/// Signal all cores.
.global cpu_send_event
cpu_send_event:
  sev
  ret
