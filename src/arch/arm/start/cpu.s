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
/// Invalidate the TLB by virtual address.
///
/// # Parameters
///
/// * r0 - The base virtual address to invalidate.
/// * r1 - The address space ID to invalidate.
///
/// # Assumptions
///
/// Assumes the base virtual address is page aligned.
.global cpu_flush_tlb_by_va
cpu_flush_tlb_by_va:
  ubfx    r0, r0, #PAGE_SHIFT, #(32 - PAGE_SHIFT)
  lsl     r0, r0, #PAGE_SHIFT
  bfi     r0, r1, #0, #8
  mcr     p15, 0, r0, c8, c7, 1
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Invalidate the TLB by virtual address range.
///
/// # Parameters
///
/// * r0 - The base virtual address to invalidate.
/// * r1 - The number of pages to invalidate.
/// * r2 - The address space ID to invalidate.
///
/// # Assumptions
///
/// Assumes the base virtual address is page aligned.
.global cpu_flush_tlb_by_va_range
cpu_flush_tlb_by_va_range:
  ubfx    r0, r0, #PAGE_SHIFT, #(32 - PAGE_SHIFT)
  lsl     r0, r0, #PAGE_SHIFT
  bfi     r0, r2, #0, #8
  mov     r3, #PAGE_SIZE

  cmp     r1, #0
  beq     2f

1:
  mcr     p15, 0, r0, c8, c7, 1
  add     r0, r0, r3
  subs    r1, r1, #1
  bne     1b

2:
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Invalidate the branch predictors by virtual address.
///
/// # Parameters
///
/// * r0 - The base virtual address to invalidate.
/// * r1 - The address space ID to invalidate.
.global cpu_flush_bp_by_va
cpu_flush_bp_by_va:
  ubfx    r0, r0, #PAGE_SHIFT, #(32 - PAGE_SHIFT)
  lsl     r0, r0, #PAGE_SHIFT
  bfi     r0, r1, #0, #8
  mcr     p15, 0, r0, c7, c5, 7
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Invalidate the branch predictors by virtual address range.
///
/// # Parameters
///
/// * r0 - The base virtual address to invalidate.
/// * r1 - The number of pages to invalidate.
/// * r2 - The address space ID to invalidate.
.global cpu_flush_bp_by_va_range
cpu_flush_bp_by_va_range:
  ubfx    r0, r0, #PAGE_SHIFT, #(32 - PAGE_SHIFT)
  lsl     r0, r0, #PAGE_SHIFT
  bfi     r0, r2, #0, #8
  mov     r3, #PAGE_SIZE

  cmp     r1, #0
  beq     2f

1:
  mcr     p15, 0, r0, c7, c5, 7
  add     r0, r0, r3
  subs    r1, r1, #1
  bne     1b

2:
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Issue a data synchronization barrier.
.global cpu_data_sync_barrier
cpu_data_sync_barrier:
  dsb
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Issue an instruction synchronization barrier.
.global cpu_instruction_sync_barrier
cpu_instruction_sync_barrier:
  isb
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Issue data and instruction synchronization barriers.
.global cpu_sync_barriers
cpu_sync_barriers:
  dsb
  isb
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Signal all cores.
.global cpu_send_event
cpu_send_event:
  sev
  mov     pc, lr
