//! AArch64 Low-Level Page Table Setup

.include "abi.h"

// Page descriptor flags. See D8.3.2. Note: Bits 58:55 are reserved for
// software use. Bit 6 is zero to deny access to EL0. Memory is RW if bit 7 is
// 0, RO otherwise.
.equ MM_TYPE_PAGE_TABLE, 0x3
.equ MM_TYPE_PAGE,       0x3
.equ MM_TYPE_BLOCK,      0x1
.equ MM_ACCESS_FLAG,     (1 << 10)
.equ MM_ACCESS_RW,       (0b00 << 6)
.equ MM_ACCESS_RO,       (0b10 << 6)

/// 2 MiB section virtual address layout:
///
///   +---------------+--------+--------+--------+--------------------+
///   | / / / / / / / |   L1   |   L2   |   L3   |       Offset       |
///   +---------------+--------+--------+--------+--------------------+
///   63             48       39       30       21                    0
///
/// 4 KiB page virtual address layout:
///
///   +---------------+--------+--------+--------+--------+-----------+
///   | / / / / / / / |   L1   |   L2   |   L3   |   L4   |  Offset   |
///   +---------------+--------+--------+--------+--------+-----------+
///   63             48       39       30       21       12           0
.equ PAGE_SHIFT,      12
.equ TABLE_SHIFT,     9
.equ TABLE_ENTRY_CNT, (1 << TABLE_SHIFT)
.equ SECTION_SHIFT,   (PAGE_SHIFT + TABLE_SHIFT)
.equ SECTION_SIZE,    (1 << SECTION_SHIFT)
.equ L2_SHIFT,        (SECTION_SHIFT + TABLE_SHIFT)
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
.equ TCR_EL1_T0SZ,   16
.equ TCR_EL1_T1SZ,   (TCR_EL1_T0SZ << 16)
.equ TCR_EL1_TG0_4K, (0 << 14)
.equ TCR_EL1_TG1_4K, (2 << 30)
.equ TCR_EL1_VALUE,  (TCR_EL1_T0SZ | TCR_EL1_T1SZ | TCR_EL1_TG0_4K | TCR_EL1_TG1_4K)

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
/// Maps the kernel and, as necessary, the DTB into 2 MiB sections. The kernel
/// will re-map the pages after determining the memory layout.
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
/// Cleanup after enabling the MMU.
///
/// # Description
///
/// Removes the identity page table from TTBR0.
.global mmu_cleanup
mmu_cleanup:
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
/// # Description
///
///   NOTE: Assumes the system is configured properly and there will be no
///         addition overflow when calculating the end address.
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
  and     x2, x2, x1

// Section align the base address.
  and     x0, x0, x1

// Calculate the new size.
  sub     x1, x2, x0

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
