//! AArch64 Low-Level DTB Utilities

// DTB magic identifier.
.equ DTB_MAGIC, 0xd00dfeed

///-----------------------------------------------------------------------------
///
/// Performs a quick check to see if the blob is a DTB.
///
/// # Parameters
///
/// * x0 - The blob address.
///
/// # Returns
///
/// The total size of the DTB or 0 if the blob is not a DTB.
.global dtb_quick_check
dtb_quick_check:
  mov     x9, x0
  mov     x0, #0

  ldr     w10, [x9]
  rev     w10, w10

  ldr     w11, =DTB_MAGIC
  cmp     w10, w11
  b.ne    1f

  ldr     w0, [x9, #4]
  rev     w0, w0

1:
  ret
