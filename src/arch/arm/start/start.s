//! ARM Entry Point

.include "abi.h"

// ARM processor modes and SPSR default. SPSR_hyp is copied to CPSR on exception
// return. The SPSR_hyp default configures HYP to return to SVC and leave all
// interrupts masked. See B1.3.1 and B1.3.3.
.equ ARM_USR_MODE,             0b10000
.equ ARM_FIQ_MODE,             0b10001
.equ ARM_IRQ_MODE,             0b10010
.equ ARM_SVC_MODE,             0b10011
.equ ARM_ABT_MODE,             0b10111
.equ ARM_HYP_MODE,             0b11010
.equ ARM_MODE_MASK,            0b11111
.equ SPSR_MASK_ALL_INTERRUPTS, (7 << 6)
.equ SPSR_HYP_DEFAULT,         (SPSR_MASK_ALL_INTERRUPTS | ARM_SVC_MODE)

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
///   NOTE: The Linux boot protocol for ARM specifies that the bootloader may
///         leave the primary core in either hypervisor or supervisor mode.
///         Hypervisor mode is preferred if the core support virtualization.
.global _start
_start:
  mov     r10, r2           // Save the blob pointer.

//----------------------------------------------------------
// TODO: This is a temporary delay loop to give OpenOCD time
//       to connect.
//----------------------------------------------------------
  ldr     r0, =0x4000000
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
  mov     r0, #SPSR_HYP_DEFAULT
  msr     spsr_hyp, r0

  eret


///-----------------------------------------------------------------------------
///
/// Entry point for SVC.
svc_entry:
  bl      cpu_get_id        // Get the core ID; core 0 is the primary core.
  cmp     r0, #0
  beq     primary_core_boot
  b       secondary_core_boot


///-----------------------------------------------------------------------------
///
/// Boot the primary core.
///
/// # Description
///
/// Per the Linux ARM boot protocol, all interrupts are masked and all other
/// cores are parked. All single-threaded kernel initialization will be done
/// here.
primary_core_boot:
// Setup the kernel's SVC stack pointer; wait on the exception stacks.
  adr     r1, kernel_svc_stack_start_rel
  ldr     r2, kernel_svc_stack_start_rel
  add     r1, r1, r2
  mov     sp, r1

// Clear the BSS. The Rust core library provides a memset compiler intrinsic.
  adr     r0, bss_start_rel
  ldr     r1, bss_start_rel
  add     r0, r0, r1
  mov     r1, #0
  ldr     r2, =__bss_size
  bl      memset

// Halt if LPAE is not supported.
  bl      ext_has_long_descriptor_support
  cmp     r0, #0
  bne     cpu_halt

// Check if the blob is a DTB. The kernel does not support ATAGs.
  mov     r0, r10
  bl      dtb_quick_check
  cmp     r0, #0
  beq     cpu_halt

// Create the bootstrap kernel page tables.
  mov     r1, r0            // DTB blob size to r1
  mov     r0, r10           // DTB blob address to r0
  ldr     r2, =__vmsplit
  bl      mmu_create_kernel_page_tables

// Save off physical addresses needed for the kernel configuration struct.
  adr     r4, kernel_start_rel
  ldr     r5, kernel_start_rel
  add     r4, r4, r5

  adr     r5, kernel_pages_start_rel
  ldr     r6, kernel_pages_start_rel
  add     r5, r5, r6

  adr     r6, kernel_svc_stack_start_rel
  ldr     r7, kernel_svc_stack_start_rel
  add     r6, r6, r7

  adr     r7, kernel_stack_list_rel
  ldr     r8, kernel_stack_list_rel
  add     r7, r7, r8

// Setup the MMU and enable it.
//
//   NOTE: Manually set the link register to the virtual return address when
//         calling `setup_and_enable_mmu`. Do not use branch-and-link.
  ldr     lr, =primary_core_begin_virt_addressing
  b       mmu_setup_and_enable
primary_core_begin_virt_addressing:
  bl      mmu_cleanup_ttbr
  bl      setup_stacks

// Write kernel configuration struct. Provide all addresses as physical.
//
//   +---------------------------------+ 44
//   | Physical primary stack address  |
//   +---------------------------------+ 40
//   | ISR stack page count            |
//   +---------------------------------+ 36
//   | ISR stack list address          |
//   +---------------------------------+ 32
//   | Virtual memory split            |
//   +---------------------------------+ 28
//   | Page table area size            |
//   +---------------------------------+ 24
//   | Physical page tables address    |
//   +---------------------------------+ 20
//   | Kernel size                     |
//   +---------------------------------+ 16
//   | Physical kernel address         |
//   +---------------------------------+ 12
//   | Physical blob address           |
//   +---------------------------------+ 8
//   | Page size                       |
//   +---------------------------------+ 4
//   | Virtual base address            |
//   +---------------------------------+ 0
  mov     fp, sp
  sub     sp, sp, #4 * 11

  ldr     r2, =__virtual_start
  str     r2, [sp, #4 * 0]

  ldr     r1, =__page_size
  str     r1, [sp, #4 * 1]

  str     r10, [sp, #4 * 2]

  str     r4, [sp, #4 * 3]

  ldr     r1, =__kernel_size
  str     r1, [sp, #4 * 4]

  str     r5, [sp, #4 * 5]

  ldr     r1, =__kernel_pages_size
  str     r1, [sp, #4 * 6]

  ldr     r1, =__vmsplit
  str     r1, [sp, #4 * 7]

  str     r6, [sp, #4 * 8]

  ldr     r1, =__kernel_stack_pages
  str     r1, [sp, #4 * 9]

  str     r7, [sp, #4 * 10]

// Perform the rest of the kernel initialization in Rustland.
  mov     r0, sp
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


///-----------------------------------------------------------------------------
///
/// Setup the kernel exception stacks using virtual addressing.
///
/// # Assumptions
///
/// Assumes the core is in SVC mode.
setup_stacks:
// Save off the CPSR
  mrs     r0, cpsr

// Set the SVC mode stack.
  ldr     sp, =__kernel_svc_stack_start

// Set the ABT mode stack.
  msr     cpsr_c, #(0b1100000 | ARM_ABT_MODE)
  ldr     sp, =__kernel_abt_stack_start

// Set the IRQ mode stack.
  msr     cpsr_c, #(0b1100000 | ARM_IRQ_MODE)
  ldr     sp, =__kernel_irq_stack_start

// Set the FIQ mode stack.
  msr     cpsr_c, #(0b1100000 | ARM_FIQ_MODE)
  ldr     sp, =__kernel_fiq_stack_start

// Reset CPSR.
  msr     cpsr, r0

  mov     pc, lr


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
kernel_svc_stack_start_rel:
  .word __kernel_svc_stack_start - kernel_svc_stack_start_rel
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
