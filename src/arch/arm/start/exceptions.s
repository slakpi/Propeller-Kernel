//! ARM Low-Level Exception Handling

// Size of the exception handler stack frame.
.equ EXCEPTION_FRAME_SIZE, 64

.equ UNDEFINED_INSTRUCTION_EXCEPTION, 1
.equ SUPERVISOR_CALL_EXCEPTION,       2
.equ PREFETCH_ABORT_EXCEPTION,        3
.equ DATA_ABORT_EXCEPTION,            4
.equ IRQ_EXCEPTION,                   5
.equ FIQ_EXCEPTION,                   6

///-----------------------------------------------------------------------------
///
/// Adds `label` as a vector to the vector table.
.macro ventry label
  ldr     pc, \label
.endm


///-----------------------------------------------------------------------------
///
/// Exception handler prologue.
///
/// # Description
///
///   NOTE: Only the integer general purpose registers are saved.
.macro kernel_entry
  sub     sp, sp, #EXCEPTION_FRAME_SIZE
  str     r0, [sp, #4 * 0]
  str     r1, [sp, #4 * 1]
  str     r2, [sp, #4 * 2]
  str     r3, [sp, #4 * 3]
  str     r4, [sp, #4 * 4]
  str     r5, [sp, #4 * 5]
  str     r6, [sp, #4 * 6]
  str     r7, [sp, #4 * 7]
  str     r8, [sp, #4 * 8]
  str     r9, [sp, #4 * 9]
  str     r10, [sp, #4 * 10]
  str     r11, [sp, #4 * 11]
  str     r12, [sp, #4 * 12]
// Skip the stack pointer.
  str     r14, [sp, #4 * 14]
  str     r15, [sp, #4 * 15] 
.endm


///-----------------------------------------------------------------------------
///
/// Exception handler epilogue.
.macro kernel_exit
  ldr     r0, [sp, #4 * 0]
  ldr     r1, [sp, #4 * 1]
  ldr     r2, [sp, #4 * 2]
  ldr     r3, [sp, #4 * 3]
  ldr     r4, [sp, #4 * 4]
  ldr     r5, [sp, #4 * 5]
  ldr     r6, [sp, #4 * 6]
  ldr     r7, [sp, #4 * 7]
  ldr     r8, [sp, #4 * 8]
  ldr     r9, [sp, #4 * 9]
  ldr     r10, [sp, #4 * 10]
  ldr     r11, [sp, #4 * 11]
  ldr     r12, [sp, #4 * 12]
// Skip the stack pointer.
  ldr     r14, [sp, #4 * 14]
  ldr     r15, [sp, #4 * 15]
  add	    sp, sp, #EXCEPTION_FRAME_SIZE
.endm


.section ".text.vectors"

///-----------------------------------------------------------------------------
///
/// Exception vector table. This page will be mapped to the high vectors page
/// at 0xffff_0000 once the MMU is up and running.
.global vectors
vectors:
  ventry  _trap_restart_addr
  ventry  _trap_undefined_instruction_addr
  ventry  _trap_supervisor_call_addr
  ventry  _trap_prefetch_abort_addr
  ventry  _trap_data_abort_addr
  nop                           // Not used
  ventry  _trap_irq_addr
  ventry  _trap_fiq_addr


_trap_restart_addr:
  .word _start
_trap_undefined_instruction_addr:
  .word _trap_undefined_instruction
_trap_supervisor_call_addr:
  .word _trap_supervisor_call
_trap_prefetch_abort_addr:
  .word _trap_prefetch_abort
_trap_data_abort_addr:
  .word _trap_data_abort
_trap_irq_addr:
  .word _trap_irq
_trap_fiq_addr:
  .word _trap_fiq


.section ".text.stubs"

///-----------------------------------------------------------------------------
///
/// Undefined instruction trap.
_trap_undefined_instruction:
  kernel_entry
  mov     r0, #UNDEFINED_INSTRUCTION_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit


///-----------------------------------------------------------------------------
///
/// Supervisor call trap.
_trap_supervisor_call:
  kernel_entry
  mov     r0, #SUPERVISOR_CALL_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit


///-----------------------------------------------------------------------------
///
/// Prefetch abort trap.
_trap_prefetch_abort:
  kernel_entry
  mov     r0, #PREFETCH_ABORT_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit


///-----------------------------------------------------------------------------
///
/// Data abort trap.
_trap_data_abort:
  kernel_entry
  mov     r0, #DATA_ABORT_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit


///-----------------------------------------------------------------------------
///
/// IRQ trap.
_trap_irq:
  kernel_entry
  mov     r0, #IRQ_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit


///-----------------------------------------------------------------------------
///
/// FIQ trap.
_trap_fiq:
  kernel_entry
  mov     r0, #FIQ_EXCEPTION
  mov     r1, sp
  bl      pk_handle_exception
  kernel_exit
