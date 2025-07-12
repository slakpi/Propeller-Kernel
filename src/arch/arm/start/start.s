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

.section ".text.boot"

///-----------------------------------------------------------------------------
///
/// Kernel entry point.
///
/// # Parameters
///
/// * r0 - Zero
/// * r1 - Machine ID
/// * r2 - Start of ATAGS
///
/// # Description
///
///   NOTE: Never returns.
///
///   TODO: Currently assuming the boot loader started the kernel with the core
///         in the SVC mode. However, per the Linux boot protocol, the boot
///         loader may leave the core in the HYP mode.
.global _start
_start:
//----------------------------------------------------------
// TODO: This is a temporary delay loop to give OpenOCD time
//       to connect.
//----------------------------------------------------------
  ldr     r0, =0x8000000
1:
  sub     r0, r0, #1
  cmp     r0, #0
  bne     1b

  b       cpu_halt


.section ".text"

/*----------------------------------------------------------------------------*/
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
