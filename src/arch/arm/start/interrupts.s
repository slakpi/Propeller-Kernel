//! ARM Low-Level Interrupt Utilities

///-----------------------------------------------------------------------------
///
/// Mask all interrupts.
.global irq_mask_all_interrupts
irq_mask_all_interrupts:
  cpsid   iaf
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Save the interrupt state and mask all interrupts.
///
/// # Returns
///
/// The state of the interrupts before masking.
.global irq_save_and_mask_all_interrupts
irq_save_and_mask_all_interrupts:
  mrs     r0, cpsr
  cpsid   iaf
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Unmask all interrupts.
.global irq_unmask_all_interrupts
irq_unmask_all_interrupts:
  cpsie   iaf
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Restore interrupt state.
///
/// # Parameters
///
/// * r0 - The original interrupt state.
///
/// # Description
///
/// See B9.3.11. Writing to CPSR_xc only writes bits [15:8] ("x" in the suffix)
/// and bits [7:0] ("c" in the suffix).
.global irq_restore_interrupt_state
irq_restore_interrupt_state:
  msr     cpsr_xc, r0
  mov     pc, lr
