//! AArch64 Low-Level Page Table Setup

.include "abi.h"

// Page descriptor flags. See D8.3.2. Note: Bits 58:55 are reserved for
// software use. Bit 6 is zero to deny access to EL0. Memory is RW if bit 7 is
// 0, RO otherwise.
.equ MM_TYPE_PAGE_TABLE, 0b11
.equ MM_TYPE_PAGE,       0b11
.equ MM_TYPE_BLOCK,      0b01
.equ MM_ACCESS_FLAG,     (0b1 << 10)
.equ MM_ACCESS_RW,       (0b00 << 6)
.equ MM_ACCESS_RO,       (0b10 << 6)

// 2 MiB section virtual address layout:
//
//   +---------------+--------+--------+--------+--------------------+
//   | / / / / / / / |   L1   |   L2   |   L3   |       Offset       |
//   +---------------+--------+--------+--------+--------------------+
//   63             48       39       30       21                    0
//
// 4 KiB page virtual address layout:
//
//   +---------------+--------+--------+--------+--------+-----------+
//   | / / / / / / / |   L1   |   L2   |   L3   |   L4   |  Offset   |
//   +---------------+--------+--------+--------+--------+-----------+
//   63             48       39       30       21       12           0
.equ PAGE_SHIFT,      12
.equ TABLE_SHIFT,     9
.equ TABLE_ENTRY_CNT, (1 << TABLE_SHIFT)
.equ SECTION_SHIFT,   (PAGE_SHIFT + TABLE_SHIFT)
.equ SECTION_SIZE,    (1 << SECTION_SHIFT)
.equ L4_SHIFT,        (PAGE_SHIFT)
.equ L3_SHIFT,        (SECTION_SHIFT)
.equ L2_SHIFT,        (L3_SHIFT + TABLE_SHIFT)
.equ L1_SHIFT,        (L2_SHIFT + TABLE_SHIFT)

// EL1 translation control register configuration.
//
// Configure the MMU to use 4 KiB granules for both the kernel and user address
// spaces.
//
// With a 4 KiB granule size, bits 47:39 of the address are the Level 1
// translation index. So, just configure T0SZ and T1SZ to mask off the top 16
// bits of the address.
//
// The kernel address space will span the 256 TiB from 0xffff_0000_0000_000 to
// 0xffff_ffff_ffff_ffff while the user address space will span the 256 TiB
// from 0x0000_0000_0000_0000 to 0x0000_ffff_ffff_ffff.
//
// TCR_EL1.IRGN1/0 and TCR_EL1.ORGN1/0 control the inner/outer cacheability for
// memory associated with the page tables expected in TTBR1/0_EL1. These tell
// the MMU whether to use the cache or read memory, so they MUST match the
// cacheability attributes referenced in MAIR_EL1. But default, no cacheing is
// assumed and the MMU will read directly from memory. Configure the MMU to
// expect normal write-back, write-allocate for both the inner and outer
// regions in TTBR1_EL1 and TTBR0_EL1.
.equ TCR_EL1_T0SZ,   16
.equ TCR_EL1_T1SZ,   (TCR_EL1_T0SZ << 16)
.equ TCR_EL1_TG0_4K, (0 << 14)
.equ TCR_EL1_TG1_4K, (2 << 30)
.equ TCR_EL1_IRGN1,  (0b01 << 24)
.equ TCR_EL1_IRGN0,  (0b01 << 8)
.equ TCR_EL1_ORGN1,  (0b01 << 26)
.equ TCR_EL1_ORGN0,  (0b01 << 10)
.equ TCR_EL1_CACHE,  (TCR_EL1_IRGN1 | TCR_EL1_IRGN0 | TCR_EL1_ORGN1 | TCR_EL1_ORGN0)
.equ TCR_EL1_VALUE,  (TCR_EL1_T0SZ | TCR_EL1_T1SZ | TCR_EL1_TG0_4K | TCR_EL1_TG1_4K | TCR_EL1_CACHE)

// EL1 memory attribute indirection register configuration. See D17.2.97.
//
//   * Configure attribute 0 to tag pages as normal memory. Inner and outer
//     write-back cacheable with allocation on read or write.
//
//   * Configure attribute 1 to tag pages as non Gathering, non Re-ordering,
//     non Early Write Acknowledgement. This is a restriction we will apply to
//     the peripheral memory to ensure writes are done exactly as specified
//     with no relative re-ordering and we get an acknowledgement from the
//     peripheral.
.equ MT_NORMAL_IDX,          0x0
.equ MT_NORMAL_SHIFT,        (MT_NORMAL_IDX << 3)
.equ MT_DEVICE_nGnRnE_IDX,   0x1
.equ MT_DEVICE_nGnRnE_SHIFT, (MT_DEVICE_nGnRnE_IDX << 3)
.equ MT_NORMAL_ATTR,         0xff
.equ MT_DEVICE_nGnRnE_ATTR,  0x00
.equ MAIR_EL1_VALUE,         ((MT_DEVICE_nGnRnE_ATTR << MT_DEVICE_nGnRnE_SHIFT) | (MT_NORMAL_ATTR << MT_NORMAL_SHIFT))

.equ MMU_NORMAL_RO_FLAGS, (MM_ACCESS_RO | (MT_NORMAL_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_NORMAL_RW_FLAGS, (MM_ACCESS_RW | (MT_NORMAL_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_DEVICE_RO_FLAGS, (MM_ACCESS_RO | (MT_DEVICE_nGnRnE_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_DEVICE_RW_FLAGS, (MM_ACCESS_RW | (MT_DEVICE_nGnRnE_IDX << 2) | MM_ACCESS_FLAG)

// EL1 MMU enable bit.
.equ SCTLR_EL1_MMU_ENABLE, (1 << 0)

///-----------------------------------------------------------------------------
///
/// Create the initial kernel page tables.
///
/// # Parameters
///
/// * x0 - The base of the blob.
/// * x1 - The size of the DTB or 0 if the blob is not a DTB.
///
/// # Description
///
/// Maps the kernel and, as necessary, the DTB into 2 MiB sections.
.global mmu_create_kernel_page_tables
mmu_create_kernel_page_tables:
  fn_entry

// x19 - The section-aligned blob base address.
// x20 - The section-aligned blob size.
// x21 - The section-aligned kernel size.
// x22 - The base address of the virtual L3 table.
// x23 - The base address of the identity L3 table.
  sub     sp, sp, #8 * 6
  stp     x19, x20, [sp, #8 * 0]
  stp     x21, x22, [sp, #8 * 2]
  stp     x23, x24, [sp, #8 * 4]

// Align the blob base and size on sections.
  bl      section_align_block
  mov     x19, x0
  mov     x20, x1

// Align the kernel size on a section.
  mov     x0, #0
  adrp    x1, __kernel_end
  bl      section_align_block
  mov     x21, x1

// Clear the page tables.
  adrp    x0, __kernel_pages_start
  mov     x1, #0
  ldr     x2, =__kernel_pages_size
  bl      memset

  adrp    x0, __kernel_id_pages_start
  mov     x1, #0
  ldr     x2, =__kernel_id_pages_size
  bl      memset

// Create the L1 and L2 page tables.
  adrp    x0, __kernel_pages_start
  ldr     x1, =__virtual_start
  bl      init_tables
  mov     x22, x0

  adrp    x0, __kernel_id_pages_start
  mov     x1, #0
  bl      init_tables
  mov     x23, x0

// Map the kernel area as RW normal memory in both the virtual and identity
// tables.
//
//   TODO: The code should probably be separate from the stack and page tables
//         to prevent the code from being re-written.
  mov     x0, x22
  mov     x1, #0
  ldr     x2, =__virtual_start
  add     x3, x2, x21
  sub     x3, x3, #1
  mov     x4, #(MMU_NORMAL_RW_FLAGS | MM_TYPE_BLOCK)
  bl      map_block

  mov     x0, x23
  mov     x1, #0
  mov     x2, #0
  add     x3, x2, x21
  sub     x3, x3, #1
  mov     x4, #(MMU_NORMAL_RW_FLAGS | MM_TYPE_BLOCK)
  bl      map_block

// Map the DTB area as RO normal memory. Skip this if the DTB size is zero.
// Do not need to create an identity map. The kernel will switch to virtual
// addresses before the DTB is needed.
  cbz     x20, skip_dtb_mapping

  mov     x0, x22
  mov     x1, #0
  add     x1, x1, x19
  ldr     x2, =__virtual_start
  add     x2, x2, x19
  add     x3, x2, x20
  sub     x3, x3, #1
  mov     x4, #(MMU_NORMAL_RO_FLAGS | MM_TYPE_BLOCK)
  bl      map_block

skip_dtb_mapping:
  ldp     x19, x20, [sp, #8 * 0]
  ldp     x21, x22, [sp, #8 * 2]
  ldp     x23, x24, [sp, #8 * 4]
  fn_exit
  ret


///-----------------------------------------------------------------------------
///
/// Create the ISR stack area tables and remap the stack pointer.
///
/// # Parameters
///
/// * x0 - Virtual start address of the primary core's ISR stack.
///
/// # Assumptions
///
/// Assumes the stack will not require multiple L4 tables.
///
/// Assumes the MMU is enabled.
///
/// Assumes that the stack is initially empty on entry.
.global mmu_setup_primary_core_stack
mmu_setup_primary_core_stack:
// x9 - Current table address
// x10 - Next table address
// x11 - Physical stack base
// x12 - Stack size
// x13 - Table index
// x14 - Page size
// x15 - Temp

  ldr     x14, =__page_size

// Set up the L1 and L2 pointers. NOTE: the MMU is on, so we will get a virtual
// address for __kernel_pages_start. It will be linearly-mapped, however, so we
// can just subtract __virtual_start.
  adrp    x9, __kernel_pages_start
  ldr     x15, =__virtual_start
  sub     x9, x9, x15
  mov     x10, x9
  add     x10, x10, x14     // Skip existing L1
  add     x10, x10, x14     // Skip existing L2
  add     x10, x10, x14     // Skip existing L3

// Get the stack size.
  ldr     x12, =__kernel_stack_pages
  mul     x12, x12, x14

// Get the virtual stack base. Preserve the virtual stack start in the frame
// pointer.
  mov     fp, x0
  sub     x0, x0, x12

// Get the L1 index.
  mov     x13, x0
  lsr     x13, x13, #L1_SHIFT
  and     x13, x13, #0x1ff

// Store the L2 pointer in the L1 table.
  mov     x15, x10
  orr     x15, x15, #MM_TYPE_PAGE_TABLE
  str     x15, [x9, x13, lsl #3]

// Set up the L2 and L3 pointers.
  mov     x9, x10
  add     x10, x10, x14

// Get the L2 index.
  mov     x13, x0
  lsr     x13, x13, #L2_SHIFT
  and     x13, x13, #0x1ff

// Store the L3 pointer in the L2 table.
  mov     x15, x10
  orr     x15, x15, #MM_TYPE_PAGE_TABLE
  str     x15, [x9, x13, lsl #3]

// Set up the L3 and L4 pointers.
  mov     x9, x10
  add     x10, x10, x14

// Get the L3 index.
  mov     x13, x0
  lsr     x13, x13, #L3_SHIFT
  and     x13, x13, #0x1ff

// Store the L4 pointer in the L3 table.
  mov     x15, x10
  orr     x15, x15, #MM_TYPE_PAGE_TABLE
  str     x15, [x9, x13, lsl #3]

// Get the first L4 index.
  mov     x13, x0
  lsr     x13, x13, #L4_SHIFT
  and     x13, x13, #0x1ff

// Set up the L4 pointer.
  mov     x9, x10

// Store the page pointers. NOTE: the MMU is on so we will get a virtual
// address for __kernel_stack_end. It too is linearly mapped, so we can just
// subtract __virtual_start.
  ldr     x11, =__kernel_stack_end
  ldr     x15, =__virtual_start
  sub     x11, x11, x15
  mov     x15, #(MMU_NORMAL_RW_FLAGS | MM_TYPE_PAGE)
  orr     x11, x11, x15
1:
  str     x11, [x9, x13, lsl #3]
  add     x13, x13, #1      // Increment table index
  add     x11, x11, x14     // Increment stack page
  sub     x12, x12, x14     // Calculate remaining stack size
  cbnz    x12, 1b           // Loop back if the stack size is not 0

// Restore the frame pointer to update the stack to the new virtual start.
  mov     sp, fp

  ret


///-----------------------------------------------------------------------------
///
/// Set the MMU flags and enable the MMU.
///
/// # Parameters
///
/// * x0 - Physical address of the Level 1 identity page table.
/// * x1 - Physical address of the Level 1 page table.
///
/// # Description
///
/// Enables the MMU with TTBR0 = x0 and TTBR1 = x1.
///
///   NOTE: The function must be called with the link register set to the
///         VIRTUAL return address.
.global mmu_setup_and_enable
mmu_setup_and_enable:
  msr     ttbr0_el1, x0
  msr     ttbr1_el1, x1

  ldr     x9, =TCR_EL1_VALUE
  msr     tcr_el1, x9

  ldr     x9, =MAIR_EL1_VALUE
  msr     mair_el1, x9

  isb
  mrs     x9, sctlr_el1
  orr     x9, x9, #SCTLR_EL1_MMU_ENABLE
  msr     sctlr_el1, x9
  isb

  ret


///-----------------------------------------------------------------------------
///
/// Cleanup the translation table registers after enabling the MMU.
///
/// # Description
///
/// Zeros out TTBR0_EL1 leaving TTBR1_EL1 with the kernel pages.
///
///   NOTE: This function will be called by the secondary cores before they have
///         stacks. This function MUST not modify callee-saved registers or call
///         other functions.
.global mmu_cleanup_ttbr
mmu_cleanup_ttbr:
  mov     x9, #0
  msr     ttbr0_el1, x9
  isb
  ret


///-----------------------------------------------------------------------------
///
/// Section-align a memory block.
///
/// # Parameters
///
/// * x0 - The base address of the block.
/// * x1 - The size of the block.
///
/// # Assumptions
///
/// Assumes the system is configured properly and there will be no addition
/// overflow when calculating the end address.
///
/// # Returns
///
/// The section-aligned base address and size.
section_align_block:
// Calculate the end address.
  add     x9, x0, x1

// Calculate the section mask.
  mov     x1, #SECTION_SIZE - 1

// Section align the end address.
  add     x9, x9, x1
  mvn     x1, x1
  and     x9, x9, x1

// Section align the base address.
  and     x0, x0, x1

// Calculate the new size.
  sub     x1, x9, x0

  ret


///-----------------------------------------------------------------------------
///
/// Initialize L1 and L2 page tables for the first 1 GiB of the physical address
/// space.
///
/// # Parameters
///
/// * x0 - The base address of the L1 table.
/// * x1 - The base address of the virtual address space.
///
/// # Returns
///
/// Returns the base address of the L3 page table.
init_tables:
  fn_entry

  mov     x2, #L1_SHIFT
  bl      create_table_entry

  mov     x2, #L2_SHIFT
  bl      create_table_entry

  fn_exit
  ret


///-----------------------------------------------------------------------------
///
/// Helper for `init_tables`. Do not call directly.
///
/// # Parameters
///
/// * x0 - The base address of the L1 or L2 table.
/// * x1 - The base address of the virtual address space.
/// * x2 - The shift specifying the L1 or L2 table.
///
/// # Returns
///
/// The address of the next page after the table.
create_table_entry:
// Shift the virtual address down and mask it with the entry count to get the
// entry index.
  lsr     x9, x1, x2
  and     x9, x9, #TABLE_ENTRY_CNT - 1

// Get the pointer to the table at the next page. Assume the address is page-
// aligned, so the offset bits are already zero.
  ldr     x10, =__page_size
  add     x10, x0, x10

// Create the entry.
  orr     x10, x10, #MM_TYPE_PAGE_TABLE
  str     x10, [x0, x9, lsl #3]

// Return the address of the next page table.
  ldr     x10, =__page_size
  add     x0, x0, x10

  ret


///-----------------------------------------------------------------------------
///
/// Map a block of 2 MiB sections to a L3 translation table.
///
/// # Parameters
///
/// * x0 - The base address of the L3 table.
/// * x1 - The base physical address.
/// * x2 - The base virtual address.
/// * x3 - The last virtual address.
/// * x4 - The entry flags.
map_block:
  lsr     x2, x2, #SECTION_SHIFT
  and     x2, x2, #TABLE_ENTRY_CNT - 1
  lsr     x3, x3, #SECTION_SHIFT
  and     x3, x3, #TABLE_ENTRY_CNT - 1
  lsr     x1, x1, #SECTION_SHIFT
  orr     x1, x4, x1, lsl #SECTION_SHIFT
1:
  str     x1, [x0, x2, lsl #3]
  add     x2, x2, #1
  add     x1, x1, #SECTION_SIZE
  cmp     x2, x3
  b.ls    1b

  ret
