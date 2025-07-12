//! AArch64 Exception Handling

// Size of the exception handler stack frame.
.equ EXCEPTION_FRAME_SIZE, 272

///-----------------------------------------------------------------------------
///
/// Adds `label` as a vector to the vector table.
///
/// # Description
///
/// The `.align` directive aligns the entries to offsets of 128 bytes.
.macro ventry label
.align 7
  b       \label
.endm


///-----------------------------------------------------------------------------
///
/// Exception handler prologue.
///
/// # Parameters
///
/// * `el` - The exception level from which the exception was taken.
///
/// # Description
///
/// If the exception was taken from EL0, SP_EL0 is saved in the context struct.
/// Otherwise, the ISR stack pointer plus the frame size is saved (the original
/// stack pointer when the exception occurred).
///
///   NOTE: Only the integer general purpose registers are saved. EL1 is
///         configured to trap floating-point and vector instructions to ensure
///         they are not used.
///
///   NOTE: We are not using the frame pointer here to restore the stack. This
///         is not a normal function.
///
/// Exception frame layout:
///
///    +------------------+ +272
///    | SPSR_EL1         |
///    +------------------+ +264
///    | ELR_EL1          |
///    +------------------+ +256
///    | SP_EL0 or SL_EL1 |
///    +------------------+ +248
///    | Frame Pointer    |
///    +------------------+ +240
///    | x29              |
///   ...                ...
///    | x0               |
///    +------------------+ +0
.macro kernel_entry el
  sub     sp, sp, #EXCEPTION_FRAME_SIZE

  stp     x0, x1, [sp, #16 * 0]
  stp     x2, x3, [sp, #16 * 1]
  stp	    x4, x5, [sp, #16 * 2]
  stp	    x6, x7, [sp, #16 * 3]
  stp	    x8, x9, [sp, #16 * 4]
  stp	    x10, x11, [sp, #16 * 5]
  stp	    x12, x13, [sp, #16 * 6]
  stp	    x14, x15, [sp, #16 * 7]
  stp	    x16, x17, [sp, #16 * 8]
  stp	    x18, x19, [sp, #16 * 9]
  stp	    x20, x21, [sp, #16 * 10]
  stp	    x22, x23, [sp, #16 * 11]
  stp	    x24, x25, [sp, #16 * 12]
  stp	    x26, x27, [sp, #16 * 13]
  stp	    x28, x29, [sp, #16 * 14]

.if \el == 0
  mrs     x21, sp_el0
.else
  add     x21, sp, #EXCEPTION_FRAME_SIZE
.endif
  mrs     x22, elr_el1
  mrs     x23, spsr_el1

  stp	    x30, x21, [sp, #16 * 15]
  stp     x22, x23, [sp, #16 * 16]
.endm


///-----------------------------------------------------------------------------
///
/// Exception handler epilogue.
///
/// # Parameters
///
/// * `el` - The exception level from which the exception was taken.
///
/// # Description
///
/// Reverses `kernel_entry` and returns from the exception.
.macro kernel_exit el
  ldp     x30, x21, [sp, #16 * 15]
  ldp     x22, x23, [sp, #16 * 16]

.if \el == 0
  msr     sp_el0, x21
.endif
  msr     elr_el1, x22
  msr     spsr_el1, x23

  ldp	    x0, x1, [sp, #16 * 0]
  ldp	    x2, x3, [sp, #16 * 1]
  ldp	    x4, x5, [sp, #16 * 2]
  ldp	    x6, x7, [sp, #16 * 3]
  ldp	    x8, x9, [sp, #16 * 4]
  ldp	    x10, x11, [sp, #16 * 5]
  ldp	    x12, x13, [sp, #16 * 6]
  ldp	    x14, x15, [sp, #16 * 7]
  ldp	    x16, x17, [sp, #16 * 8]
  ldp	    x18, x19, [sp, #16 * 9]
  ldp	    x20, x21, [sp, #16 * 10]
  ldp	    x22, x23, [sp, #16 * 11]
  ldp	    x24, x25, [sp, #16 * 12]
  ldp	    x26, x27, [sp, #16 * 13]
  ldp	    x28, x29, [sp, #16 * 14]

  add	    sp, sp, #EXCEPTION_FRAME_SIZE

  eret
.endm


///-----------------------------------------------------------------------------
///
/// Exception vector table.
///
/// # Description
///
/// Aligned to 2 KiB. See D1.3.1.
.align 11
.global el1_vectors
el1_vectors:
// Exception taken from EL1 with SP_EL0
  ventry  _trap_exception_el1   // Synchronous
  ventry  _trap_exception_el1   // IRQ
  ventry  _trap_exception_el1   // FIQ
  ventry  _trap_exception_el1   // Error

// Exception taken from EL1 with SP_EL1
  ventry  _trap_exception_el1
  ventry  _trap_exception_el1
  ventry  _trap_exception_el1
  ventry  _trap_exception_el1

// Exception taken from EL0 in AArch64
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0

// Exception taken from EL0 in AArch32
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0
  ventry  _trap_exception_el0


///-----------------------------------------------------------------------------
///
/// Default exception trap stub for EL0.
///
/// # Description
///
/// On entry, the stack pointer is SP_EL1 (the current core's ISR stack). If we
/// take the exception from EL0, we need to preserve SP_EL0 instead of the
/// current SP.
_trap_exception_el0:
  kernel_entry 0
  mrs     x0, esr_el1
  mrs     x1, far_el1
  mov     x2, sp
  bl      pk_handle_exception // Transfer to Rustland
  kernel_exit 0


///-----------------------------------------------------------------------------
///
/// Default exception trap stub for EL1.
_trap_exception_el1:
  kernel_entry 1
  mrs     x0, esr_el1
  mrs     x1, far_el1
  mov     x2, sp
  bl      pk_handle_exception // Transfer to Rustland
  kernel_exit 1
