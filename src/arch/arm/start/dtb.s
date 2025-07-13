//! ARM Low-Level DTB Utilities

// DTB magic identifier.
.equ DTB_MAGIC, 0xd00dfeed

///-----------------------------------------------------------------------------
///
/// Performs a quick check to see if the blob is a DTB.
///
/// # Parameters
///
/// * r0 - The blob address.
///
/// # Returns
///
/// The total size of the DTB or 0 if the blob is not a DTB.
.global dtb_quick_check
dtb_quick_check:
  mov     r1, r0
  mov     r0, #0

  ldr     r2, [r1]
  rev     r2, r2

  ldr     r3, =DTB_MAGIC
  cmp     r2, r3
  bne     1f

  ldr     r0, [r1, #4]
  rev     r0, r0

1:
  mov     pc, lr
