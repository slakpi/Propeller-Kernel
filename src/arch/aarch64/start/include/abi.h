//! AArch64 ABI Macros

///-----------------------------------------------------------------------------
///
/// Implements the AArch64 calling convention entry bookkeeping.
.macro fn_entry
  stp     x29, x30, [sp, #-16]!
  mov     x29, sp
.endm


///-----------------------------------------------------------------------------
///
/// Implements the AArch64 calling convention exit bookkeeping.
.macro fn_exit
  mov     sp, x29
  ldp     x29, x30, [sp], #16
.endm
