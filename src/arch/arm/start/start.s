//! ARM Start

.include "abi.h"

// SCTLR flags. See B4.1.130. Enable the MMU, expect exception vectors at the
// high address (0xffff_0000), enable the Access Flag, enable data caching.
.equ SCTLR_MMU_ENABLE, 1
.equ SCTLR_C,          (0b1 << 2)
.equ SCTLR_V,          (0b1 << 13)
.equ SCTLR_AFE,        (0b1 << 29)
.equ SCTLR_FLAGS,      (SCTLR_MMU_ENABLE | SCTLR_AFE | SCTLR_V | SCTLR_C)

// DACR setup. See B4.1.43. Only using domain 0 in client mode (access
// permissions are checked).
.equ DACR_VALUE, 0b1

// ARM processor modes.
.equ ARM_HYP_MODE,  0b11010
.equ ARM_SVC_MODE,  0b10011
.equ ARM_MODE_MASK, 0b11111

.section ".text.boot"

///-----------------------------------------------------------------------------
///
/// Kernel entry point.
///
/// # Parameters
///
/// * r0 - Zero
/// * r1 - Machine ID
/// * r2 - Pointer to the ATAG/DTB blob (primary core)
///
/// # Description
///
///   NOTE: Never returns.
///
///   NOTE: The Linux boot protocol for ARM specifies that the bootloader may
///         leave the primary core in either hypervisor or supervisor mode.
///         Hypervisor mode is preferred if the core support virtualization.
.global _start
_start:
  mov     r8, r2            // Save the blob pointer.

//----------------------------------------------------------
// TODO: This is a temporary delay loop to give OpenOCD time
//       to connect.
//----------------------------------------------------------
  ldr     r0, =0x8000000
1:
  sub     r0, r0, #1
  cmp     r0, #0
  bne     1b

// Initialize the kernel modes as necessary to get to SVC mode.
  mov     r0, #ARM_MODE_MASK
  mrs     r1, cpsr          // Read the CPSR.
  and     r0, r0, r1        // Get the mode bits.

  cmp     r0, #ARM_HYP_MODE
  beq     hyp_entry         // Initialize HYP and SVC mode
  cmp     r0, #ARM_SVC_MODE
  beq     svc_entry         // Initialize SVC mode

  b       cpu_halt          // Unknown state


.section ".text"

///-----------------------------------------------------------------------------
///
/// Entry point for HYP.
hyp_entry:
// Set the exception return address in ELR_hyp.
  adr     r0, svc_entry_rel
  ldr     r1, svc_entry_rel
  add     r0, r0, r1
  msr     elr_hyp, r0

// Set the exception return mode to SVC in SPSR_hyp.
  mrs     r0, spsr_hyp
  bic     r0, r0, #ARM_MODE_MASK
  orr     r0, r0, #ARM_SVC_MODE
  msr     spsr_hyp, r0

  eret


///-----------------------------------------------------------------------------
///
/// Entry point for SVC.
svc_entry:
  b       cpu_halt


///-----------------------------------------------------------------------------
///
/// The ARM toolchain does not support the ADRP pseudo-instruction that allows
/// getting the 4 KiB page, PC-relative address of a label within +/- 4 GiB. ADR
/// only allows getting the PC-relative address of a label within +/- 1 MiB.
///
/// We create these "relative" labels marking address that are offsets to the
/// symbols we need. We can use ADR to get the PC-relative address of the label,
/// then add the value at the label to get the PC-relative address of the actual
/// label we're interested in.
///
/// Once the MMU has been enabled, these are no longer necessary since the LDR
/// instruction can be used to get the virtual address of the label.
kernel_start_rel:
  .word __kernel_start - kernel_start_rel
kernel_stack_start_rel:
  .word __kernel_stack_start - kernel_stack_start_rel
kernel_stack_list_rel:
  .word __kernel_stack_list - kernel_stack_list_rel
kernel_id_pages_start_rel:
  .word __kernel_id_pages_start - kernel_id_pages_start_rel
kernel_pages_start_rel:
  .word __kernel_pages_start - kernel_pages_start_rel
bss_start_rel:
  .word __bss_start - bss_start_rel
svc_entry_rel:
  .word svc_entry - svc_entry_rel
