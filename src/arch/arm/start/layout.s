//! ARM Physical Layout Utilities
//!
//! The ARM toolchain does not support the ADRP pseudo-instruction that allows
//! getting the 4 KiB page, PC-relative address of a label within +/- 4 GiB. ADR
//! only allows getting the PC-relative address of a label within +/- 1 MiB.
//!
//! Instead of getting the physical address of a label, we create another
//! "relative" label that is within +/- 4 KiB of the referencing code. The
//! difference between the "relative" label and actual address is stored at the
//! "relative" label. Using ADR on the "relative" label gets the physical
//! near PC-relative address of the label, and using LDR on the "relative" label
//! gets the difference value. Adding the two calculates the actual physical
//! address.
//!
//! Once the MMU has been enabled, these are no longer necessary since the LDR
//! instruction can be used to get the virtual address of the label.

///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the kernel.
.global layout_get_physical_kernel_start
layout_get_physical_kernel_start:
  adr     r0, kernel_start_rel
  ldr     r1, kernel_start_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_start_rel:
  .word __kernel_start - kernel_start_rel


///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the BSS area.
.global layout_get_physical_bss_start
layout_get_physical_bss_start:
  adr     r0, bss_start_rel
  ldr     r1, bss_start_rel
  add     r0, r0, r1
  mov     pc, lr


bss_start_rel:
  .word __bss_start - bss_start_rel


///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the stack list table.
.global layout_get_physical_stack_list
layout_get_physical_stack_list:
  adr     r0, kernel_stack_list_rel
  ldr     r1, kernel_stack_list_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_stack_list_rel:
  .word __kernel_stack_list - kernel_stack_list_rel


///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the exception vectors table.
.global layout_get_physical_exception_vectors_start
layout_get_physical_exception_vectors_start:
  adr     r0, kernel_exception_vectors_start_rel
  ldr     r1, kernel_exception_vectors_start_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_exception_vectors_start_rel:
  .word __kernel_exception_vectors_start - kernel_exception_vectors_start_rel


///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the identity page tables.
.global layout_get_physical_id_pages_start
layout_get_physical_id_pages_start:
  adr     r0, kernel_id_pages_start_rel
  ldr     r1, kernel_id_pages_start_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_id_pages_start_rel:
  .word __kernel_id_pages_start - kernel_id_pages_start_rel


///-----------------------------------------------------------------------------
///
/// Get the ending physical address of the identity page tables.
.global layout_get_physical_id_pages_end
layout_get_physical_id_pages_end:
  adr     r0, kernel_id_pages_end_rel
  ldr     r1, kernel_id_pages_end_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_id_pages_end_rel:
  .word __kernel_id_pages_end - kernel_id_pages_end_rel


///-----------------------------------------------------------------------------
///
/// Get the starting physical address of the page tables.
.global layout_get_physical_pages_start
layout_get_physical_pages_start:
  adr     r0, kernel_pages_start_rel
  ldr     r1, kernel_pages_start_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_pages_start_rel:
  .word __kernel_pages_start - kernel_pages_start_rel


///-----------------------------------------------------------------------------
///
/// Get the ending physical address of the page tables.
.global layout_get_physical_pages_end
layout_get_physical_pages_end:
  adr     r0, kernel_pages_end_rel
  ldr     r1, kernel_pages_end_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_pages_end_rel:
  .word __kernel_pages_end - kernel_pages_end_rel


///-----------------------------------------------------------------------------
///
/// Get the ending physical address of the kernel.
.global layout_get_physical_kernel_end
layout_get_physical_kernel_end:
  adr     r0, kernel_end_rel
  ldr     r1, kernel_end_rel
  add     r0, r0, r1
  mov     pc, lr


kernel_end_rel:
  .word __kernel_end - kernel_end_rel
