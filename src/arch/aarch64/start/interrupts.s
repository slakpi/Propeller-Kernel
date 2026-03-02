//! AArch64 Low-Level Interrupt Utilities

///-----------------------------------------------------------------------------
///
/// Mask all interrupts.
.global irq_mask_all_interrupts
irq_mask_all_interrupts:
  msr     daifset, #0b1111
  ret


///-----------------------------------------------------------------------------
///
/// Save the interrupt state and mask all interrupts.
///
/// # Returns
///
/// The state of the interrupts before masking.
.global irq_save_and_mask_all_interrupts
irq_save_and_mask_all_interrupts:
  mrs     x0, daif
  msr     daifset, #0b1111
  ret


///-----------------------------------------------------------------------------
///
/// Unmask all interrupts.
.global irq_unmask_all_interrupts
irq_unmask_all_interrupts:
  msr     daifclr, #0b1111
  ret


///-----------------------------------------------------------------------------
///
/// Restore interrupt state.
///
/// # Parameters
///
/// * x0 - The original interrupt state.
.global irq_restore_interrupt_state
irq_restore_interrupt_state:
  msr     daif, x0
  ret
