//! ARM Low-Level Architecture Extension Utilities

.equ ID_MMFR0_VMSA_MASK,           0xf
.equ VMSAv7_WITH_LONG_DESCRIPTORS, 5

///-----------------------------------------------------------------------------
///
/// Check for long page table descriptor support.
///
/// # Returns
///
/// 0 if the CPU supports long page table descriptors, non-zero otherwise.
.global ext_has_long_descriptor_support
ext_has_long_descriptor_support:
  mrc     p15, 0, r0, c0, c1, 4
  and     r0, r0, #ID_MMFR0_VMSA_MASK
  sub     r0, r0, #VMSAv7_WITH_LONG_DESCRIPTORS
  
  mov     pc, lr
