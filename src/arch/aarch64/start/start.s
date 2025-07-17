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

// Saved program status register defaults. SPSR_ELx is copied to PSTATE for the
// lower exception level when returning from the exception. The SPSR_EL3 default
// configures EL2 to use SP_EL2 after the exception return from EL3. Likewise,
// the SPSR_EL2 default configures EL1 to use SP_EL1. SPSR_EL1 does not need to
// be configured. See C5.2.19, C5.2.20, and D1.2.2.
.equ SPSR_MASK_ALL_INTERRUPTS, (7 << 6)
.equ SPSR_EL3_SP,              (9 << 0)
.equ SPSR_EL2_SP,              (5 << 0)
.equ SPSR_EL3_DEFAULT,         (SPSR_MASK_ALL_INTERRUPTS | SPSR_EL3_SP)
.equ SPSR_EL2_DEFAULT,         (SPSR_MASK_ALL_INTERRUPTS | SPSR_EL2_SP)

// EL1 system control register default. Set the required reserved bits to 1 per
// D17.2.118. Leave EL1 and EL0 in little endian and leave the MMU disabled.
.equ SCTLR_EL1_C,          (1 << 2)
.equ SCTLR_EL1_RESERVED,   ((3 << 28) | (3 << 22) | (1 << 20) | (1 << 11) | (3 << 7))
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
///
/// # Description
///
///   NOTE: The Linux AArch64 boot protocol requires the bootloader to leave the
///         primary core in either EL2 or EL1. EL2 is preferred if the core
///         supports virtualization.
.global _start
_start:
  mov     w19, w0           // Save the blob pointer.

//----------------------------------------------------------
// TODO: This is a temporary delay loop to give OpenOCD time
//       to connect.
//----------------------------------------------------------
  ldr     x0, =0x4000000
1:
  sub     x0, x0, #1
  cbnz    x0, 1b

// Initialize the exception levels as needed to get to EL1.
  mrs     x9, CurrentEL
  lsr     x9, x9, #2        // Bits 3:2 are the exception level

  cmp     x9, #3
  beq     el3_entry         // Initialize EL3, EL2, and EL1
  cmp     x9, #2
  beq     el2_entry         // Initialize EL2 and EL1
  cmp     x9, #1
  beq     el1_entry         // Initialize EL1

  b       cpu_halt          // Unknown state


.section ".text"

///-----------------------------------------------------------------------------
///
/// Entry point for EL3.
el3_entry:
  ldr     x9, =SCR_EL3_DEFAULT
  msr     scr_el3, x9

  ldr     x9, =SPSR_EL3_DEFAULT
  msr     spsr_el3, x9

  adr     x9, el2_entry
  msr     elr_el3, x9

  eret


///-----------------------------------------------------------------------------
///
/// Entry point for EL2.
el2_entry:
  ldr     x9, =HCR_EL2_DEFAULT
  msr     hcr_el2, x9

  ldr     x9, =SPSR_EL2_DEFAULT
  msr     spsr_el2, x9

  adr     x9, el1_entry
  msr     elr_el2, x9

  eret


///-----------------------------------------------------------------------------
///
/// Entry point for EL1.
el1_entry:
  ldr     x9, =SCTLR_EL1_DEFAULT
  msr     sctlr_el1, x9

  bl      cpu_get_id        // Get the core ID; core 0 is the primary core.
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
// EL1 stack setup before turning on the MMU.
  adrp    x0, __kernel_stack_start
  mov     sp, x0
  mov     fp, sp

// Clear the BSS. The Rust core library provides a memset compiler intrinsic.
  adrp    x0, __bss_start
  mov     x1, #0
  ldr     x2, =__bss_size
  bl      memset

// Check if the blob is a DTB. The kernel does not support ATAGs.
  mov     x0, x19
  bl      dtb_quick_check
  cbz     x0, cpu_halt

// Create the bootstrap kernel page tables.
  mov     x1, x0            // DTB blob size to x1
  mov     x0, x19           // DTB blob address to x0
  bl      mmu_create_kernel_page_tables

// Save off physical addresses needed for the kernel configuration struct.
  adrp    x20, __kernel_start
  adrp    x21, __kernel_pages_start
  adrp    x22, __kernel_stack_start
  adrp    x23, __kernel_stack_list

// Enable the MMU.
//
//   NOTE: Manually set the link register to the virtual return address when
//         calling `mmu_setup_and_enable`. Do not use branch-and-link.
  adrp    x0, __kernel_id_pages_start
  adrp    x1, __kernel_pages_start
  ldr     lr, =primary_core_begin_virt_addressing
  b       mmu_setup_and_enable
primary_core_begin_virt_addressing:
  bl      mmu_cleanup

// Setup the exception vectors.
  adr     x9, el1_vectors
  msr     vbar_el1, x9

// ISR stack setup with virtual addressing enabled.
  ldr     x9, =__kernel_stack_start
  mov     sp, x9

// Write kernel configuration struct. Provide all addresses as physical.
//
//   +---------------------------------+ 80
//   | Physical primary stack address  |
//   +---------------------------------+ 72
//   | ISR stack page count            |
//   +---------------------------------+ 64
//   | ISR stack list address          |
//   +---------------------------------+ 56
//   | Page table area size            |
//   +---------------------------------+ 48
//   | Physical page tables address    |
//   +---------------------------------+ 40
//   | Kernel size                     |
//   +---------------------------------+ 32
//   | Physical kernel address         |
//   +---------------------------------+ 24
//   | Physical blob address           |
//   +---------------------------------+ 16
//   | Page size                       |
//   +---------------------------------+ 8
//   | Virtual base address            |
//   +---------------------------------+ 0
  mov     fp, sp
  sub     sp, sp, #8 * 10

  ldr     x9, =__virtual_start
  ldr     x10, =__page_size
  stp     x9, x10, [sp, #8 * 0]

  stp     x19, x20, [sp, #8 * 2]

  ldr     x9, =__kernel_size
  stp     x9, x21, [sp, #8 * 4]

  ldr     x9, =__kernel_pages_size
  stp     x9, x23, [sp, #8 * 6]

  ldr     x9, =__kernel_stack_pages
  stp     x9, x23, [sp, #8 * 8]

// Perform single-threaded kernel initialization.
  mov     x0, sp
  bl      pk_init

// Clear the configuration struct and jump to the scheduler.
  mov     sp, fp
  b       pk_scheduler

// We will never return from the scheduler.


///-----------------------------------------------------------------------------
///
/// Boot a secondary core.
secondary_core_boot:
  b       cpu_halt
