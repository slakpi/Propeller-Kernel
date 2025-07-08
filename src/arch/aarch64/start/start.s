//! AArch64 Entry Point

// EL3 secure configuration default. Levels lower than EL3 are not secure and
// cannot access secure memory. EL2 uses AArch64; EL1 is controlled by HCR_EL2.
// See D17.2.117.
.equ SCR_EL3_NS,      (1 <<  0)
.equ SCR_EL3_RW,      (1 << 10)
.equ SCR_EL3_DEFAULT, (SCR_EL3_RW | SCR_EL3_NS)

// EL2 hypervisor configuration default. EL1 uses AArch64; EL0 is controlled by
// PSTATE. See D17.2.48.
.equ HCR_EL2_RW,      (1 << 31)
.equ HCR_EL2_DEFAULT, (HCR_EL2_RW)

// Saved program status register defaults. These are only going to be used when
// jumping from EL3 -> EL2 and EL2 -> EL1. We are going to make sure interrupts
// remain masked. SP_EL2 will be used when jumping to EL1 and SP_EL1 will be
// used when jumping to EL1. See C5.2.19 and C5.2.20.
.equ SPSR_MASK_ALL_INTERRUPTS, (7 << 6)
.equ SPSR_EL3_SP,              (9 << 0)
.equ SPSR_EL2_SP,              (5 << 0)
.equ SPSR_EL3_DEFAULT,         (SPSR_MASK_ALL_INTERRUPTS | SPSR_EL3_SP)
.equ SPSR_EL2_DEFAULT,         (SPSR_MASK_ALL_INTERRUPTS | SPSR_EL2_SP)

// EL1 system control register default. Set the required reserved bits to 1 per
// D17.2.118. Leave EL1 and EL0 in little endian and leave the MMU disabled.
.equ SCTLR_EL1_C,          (1 << 2)
.equ SCTLR_EL1_RESERVED,   ((3 << 28) | (3 << 22) | (1 << 20) | (1 << 11) | (3 << 7))
.equ SCTLR_EL1_MMU_ENABLE, (1 << 0)
.equ SCTLR_EL1_DEFAULT,    (SCTLR_EL1_RESERVED | SCTLR_EL1_C)

.section ".text.boot"

///-----------------------------------------------------------------------------
///
/// Kernel entry point.
///
/// # Parameters
///
/// * w0 - 32-bit pointer to the ATAG/DTB blob (primary core)
/// * x1 - Zero
/// * x2 - Zero
/// * x3 - Zero
/// * x4 - Address of this entry point.
.global _start
_start:
  mov     w19, w0           // Save the blob pointer.

//----------------------------------------------------------
// TODO: This is a temporary delay loop to give OpenOCD time
//       to connect.
//----------------------------------------------------------
  ldr     x0, =0x10000000
1:
  sub     x0, x0, #1
  cbnz    x0, 1b

  bl      init_kernel_el    // Prepare to jump down to EL1.
  eret                      // Jump down to the next EL.


.section ".text"

///-----------------------------------------------------------------------------
///
/// Initialize the kernel in the correct exception level.
init_kernel_el:
  mrs     x9, CurrentEL
  lsr     x9, x9, #2

  cmp     x9, #1
  beq     1f                // Skip EL2 initialization if already in EL1
  cmp     x9, #2
  beq     2f                // Skip EL3 initialization if already in EL2

3:
  ldr     x9, =SCR_EL3_DEFAULT
  msr     scr_el3, x9

  ldr     x9, =SPSR_EL3_DEFAULT
  msr     spsr_el3, x9

  adr     x9, el2_entry
  msr     elr_el3, x9

2:
  ldr     x9, =HCR_EL2_DEFAULT
  msr     hcr_el2, x9

  ldr     x9, =SPSR_EL2_DEFAULT
  msr     spsr_el2, x9

  adr     x9, el1_entry
  msr     elr_el2, x9

1:
  ldr     x9, =SCTLR_EL1_DEFAULT
  msr     sctlr_el1, x9

  ret


///-----------------------------------------------------------------------------
///
/// Dummy entry point for EL3 -> E2.
el2_entry:
  eret


///-----------------------------------------------------------------------------
///
/// Entry point for EL2 -> EL1.
el1_entry:
  mrs     x0, mpidr_el1     // Get the core ID; core 0 is the primary core.
  and     x0, x0, #0xff
  cbz     x0, primary_core_boot
  b       secondary_core_boot


///-----------------------------------------------------------------------------
///
/// Boot the primary core.
///
/// # Description
///
/// Per the Linux AArch64 boot protocol, all interrupts are masked and all other
/// cores are parked. All single-threaded kernel initialization will be done
/// here.
primary_core_boot:
  b       cpu_halt


///-----------------------------------------------------------------------------
///
/// Boot a secondary core.
secondary_core_boot:
  b       cpu_halt
