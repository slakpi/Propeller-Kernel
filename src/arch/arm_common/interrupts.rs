//! ARM Interrupt Management

unsafe extern "C" {
  fn irq_mask_all_interrupts();
  fn irq_save_and_mask_all_interrupts() -> usize;
  fn irq_unmask_all_interrupts();
  fn irq_restore_interrupt_state(irq_state: usize);
}

/// Mask all interrupts for the current core.
pub fn mask_all_interrupts() {
  unsafe { irq_mask_all_interrupts() };
}

/// Save the interrupt state and mask all interrupts.
///
/// # Returns
///
/// The state of the interrupts before masking.
pub fn save_and_mask_all_interrupts() -> usize {
  unsafe { irq_save_and_mask_all_interrupts() }
}

/// Unmask all interrupts for the current core.
pub fn unmask_all_interrupts() {
  unsafe { irq_unmask_all_interrupts() };
}

/// Restore interrupt state.
///
/// # Parameters
///
/// * `irq_state` - The original interrupt state.
///
/// # Description
///
///   NOTE: This function is common to ARM and AArch64, so `irq_state` must be
///         a value returned from `save_and_mask_all_interrupts()`.
pub fn restore_interrupt_state(irq_state: usize) {
  unsafe { irq_restore_interrupt_state(irq_state) }
}
