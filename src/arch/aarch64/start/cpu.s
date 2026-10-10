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
  ret


///-----------------------------------------------------------------------------
///
/// Invalidate the TLB by virtual address.
///
/// # Parameters
///
/// * x0 - The base virtual address to invalidate.
/// * x1 - The address space ID to invalidate.
.global cpu_flush_tlb_by_va
cpu_flush_tlb_by_va:
  ubfx    x0, x0, #PAGE_SHIFT, #(48 - PAGE_SHIFT)
  bfi     x0, x1, #48, #16
  tlbi    vae1, x0
  ret


///-----------------------------------------------------------------------------
///
/// Invalidate the TLB by virtual address range.
///
/// # Parameters
///
/// * x0 - The base virtual address to invalidate.
/// * x1 - The number of pages to invalidate.
/// * x2 - The address space ID to invalidate.
.global cpu_flush_tlb_by_va_range
cpu_flush_tlb_by_va_range:
  ubfx    x0, x0, #PAGE_SHIFT, #(48 - PAGE_SHIFT)
  bfi     x0, x2, #48, #16
  mov     x3, #PAGE_SIZE

  cmp     x1, #0
  b.eq    2f

1:
  tlbi    vae1, x0
  add     x0, x0, x3
  subs    x1, x1, #1
  b.ne    1b

2:
  ret


///-----------------------------------------------------------------------------
///
/// Issue a data synchronization barrier.
.global cpu_data_sync_barrier
cpu_data_sync_barrier:
  dsb     sy
  ret


///-----------------------------------------------------------------------------
///
/// Issue an instruction synchronization barrier.
.global cpu_instruction_sync_barrier
cpu_instruction_sync_barrier:
  isb     sy
  ret


///-----------------------------------------------------------------------------
///
/// Issue data and instruction synchronization barriers.
.global cpu_sync_barriers
cpu_sync_barriers:
  dsb     sy
  isb     sy
  ret


///-----------------------------------------------------------------------------
///
/// Signal all cores.
.global cpu_send_event
cpu_send_event:
  sev
  ret
