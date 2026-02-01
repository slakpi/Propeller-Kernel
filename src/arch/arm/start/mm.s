//! ARM Low-Level Page Table Setup

.include "abi.h"

// TTBCR value. See B4.1.153 and B3.6.4. A value of 0 for TTBCR.A1 tells the
// MMU that TTBR0 defines address space IDs. TTBCR.EAE enable extended address
// extensions for long page descriptors. TTBCR.T0SZ is 0 to let the user
// segment fill the virtual addresses not used by the kernel segment.
// TTBCR.T1SZ is either 1 for a 2/2 split or 2 for a 3/1 split.
//
// TTBCR.IRGN1/0 and TTBCR.ORGN1/0 control the inner/outer cacheability for
// memory associated with the page tables expected in TTBR1/0. These tell the
// MMU whether to use the cache or read memory, so they MUST match the
// cacheability attributes referenced in MAIR. But default, no cacheing is
// assumed and the MMU will read directly from memory. Configure the MMU to
// expect normal write-back, write-allocate for both the inner and outer
// regions in TTBR1 and TTBR0.
.equ TTBCR_EAE,    (0x1 << 31)
.equ TTBCR_A1,     (0x0 << 22)
.equ TTBCR_T1SZ_2, (0x1 << 16)
.equ TTBCR_T1SZ_3, (0x2 << 16)
.equ TTBCR_T0SZ,   (0x0 << 0)
.equ TTBCR_IRGN1,  (0b01 << 24)
.equ TTBCR_IRGN0,  (0b01 << 8)
.equ TTBCR_ORGN1,  (0b01 << 26)
.equ TTBCR_ORGN0,  (0b01 << 10)
.equ TTBCR_CACHE,  (TTBCR_IRGN1 | TTBCR_IRGN0 | TTBCR_ORGN1 | TTBCR_ORGN0)
.equ TTBCR_VALUE,  (TTBCR_EAE | TTBCR_A1 | TTBCR_T0SZ | TTBCR_CACHE)

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

// Page descriptor flags. See B3.6.1, B3.6.2, and B4.1.104.
.equ MM_TYPE_PAGE_TABLE, 0b11
.equ MM_TYPE_PAGE,       0b11
.equ MM_TYPE_BLOCK,      0b01
.equ MM_ACCESS_FLAG,     (0b1 << 10)
.equ MM_ACCESS_RW,       (0b00 << 6)
.equ MM_ACCESS_RO,       (0b10 << 6)

// Memory attribute indirection register configuration. See B4.1.104.
//
//   * Configure attribute 0 to tag pages as normal memory. Inner and outer
//     write-back cacheable with allocation on read or write.
//
//   * Configure attribute 1 to tag pages as device memory.
.equ MT_NORMAL_IDX,   0x0
.equ MT_NORMAL_SHIFT, (MT_NORMAL_IDX << 3)
.equ MT_DEVICE_IDX,   0x1
.equ MT_DEVICE_SHIFT, (MT_DEVICE_IDX << 3)
.equ MT_NORMAL_ATTR,  0xff
.equ MT_DEVICE_ATTR,  0x04
.equ MAIR0_VALUE,     ((MT_DEVICE_ATTR << MT_DEVICE_SHIFT) | (MT_NORMAL_ATTR << MT_NORMAL_SHIFT))
.equ MAIR1_VALUE,     0

.equ MMU_NORMAL_RO_FLAGS, (MM_ACCESS_RO | (MT_NORMAL_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_NORMAL_RW_FLAGS, (MM_ACCESS_RW | (MT_NORMAL_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_DEVICE_RO_FLAGS, (MM_ACCESS_RO | (MT_DEVICE_IDX << 2) | MM_ACCESS_FLAG)
.equ MMU_DEVICE_RW_FLAGS, (MM_ACCESS_RW | (MT_DEVICE_IDX << 2) | MM_ACCESS_FLAG)

.equ RECURSIVE_L2_OFFSET, 0xff0
.equ VECTORS_L2_OFFSET,   0xff8
.equ VECTORS_L3_OFFSET,   0xf80

// 2 MiB section virtual address layout:
//
//   +----+--------+--------------------+
//   | L1 |   L2   |       Offset       |
//   +----+--------+--------------------+
//   31  30       21                    0
//
// 4 KiB page virtual address layout:
//
//   +----+--------+--------+-----------+
//   | L1 |   L2   |   L3   |  Offset   |
//   +----+--------+--------+-----------+
//   31  30       21       12           0
.equ PAGE_SHIFT,         12
.equ L1_TABLE_SHIFT,     2
.equ L2_TABLE_SHIFT,     9
.equ L3_TABLE_SHIFT,     9
.equ L1_TABLE_ENTRY_CNT, (1 << L1_TABLE_SHIFT)
.equ L2_TABLE_ENTRY_CNT, (1 << L2_TABLE_SHIFT)
.equ L3_TABLE_ENTRY_CNT, (1 << L3_TABLE_SHIFT)
.equ SECTION_SHIFT,      (PAGE_SHIFT + L3_TABLE_SHIFT)
.equ SECTION_SIZE,       (1 << SECTION_SHIFT)
.equ L3_SHIFT,           (PAGE_SHIFT)
.equ L2_SHIFT,           (L3_SHIFT + L3_TABLE_SHIFT)
.equ L1_SHIFT,           (L2_SHIFT + L2_TABLE_SHIFT)

///-----------------------------------------------------------------------------
///
/// Create the initial kernel page tables using long descriptors.
///
/// # Parameters
///
/// * r0 - The base of the blob.
/// * r1 - The size of the DTB or 0 if the blob is not a DTB.
/// * r2 - The virtual memory split.
///
/// # Description
///
///   TODO: Break this monolith up.
///
/// Maps the kernel and, as necessary, the DTB into 2 MiB sections. The kernel
/// will re-map the pages after determining the memory layout.
///
/// The mapping will use LPAE and long page table descriptors. The start code
/// should have already set the TTBR0/TTBR1 split. This code only needs to know
/// the virtual base address to choose the correct L1 table entry.
.global mmu_create_kernel_page_tables
mmu_create_kernel_page_tables:
  fn_entry
  
// r4 - Scratch.
// r5 - The section-aligned kernel size.
// r6 - The saved blob base.
// r7 - The section-aligned blob size.
// r8 - The saved virtual memory split.
// r9 - Lower L2 table address.
// r10 - Upper L2 table address.
  push    {r4, r5, r6, r7, r8, r9, r10}

  mov     r8, r2

// Align the blob base and size on sections.
  bl      section_align_block
  mov     r6, r0
  mov     r7, r1

// Align the kernel size on a section.
  bl      layout_get_physical_kernel_end
  mov     r1, r0
  mov     r0, #0
  bl      section_align_block
  mov     r5, r1

// Initialize the indirect memory attributes
  bl      init_mair

// Clear the kernel page tables; save the start address.
  bl      layout_get_physical_pages_start
  mov     r9, r0
  mov     r1, #0
  ldr     r2, =__kernel_pages_size
  bl      memset

// Initialize the kernel page tables. If using a 3/1 split, translation through
// TTBR1 will start at a L2 table. If using a 2/2 split, however, both TTBR0 and
// TTBR1 will start at a L1 table. Check the virtual memory split value. If 3,
// skip initializing an L1 table and use the start address as the L2 table.
  mov     r0, r9
  mov     r1, r9            // Use the same L2 table for the kernel and vectors.
  cmp     r8, #3            // Using 3/1 split?
  beq     1f                // If yes, skip L1 initialization
  ldr     r1, =__virtual_start
  bl      init_table
  
1:
  mov     r9, r0            // Save the lower L2 table address
  mov     r10, r1           // Save the upper L2 table address

// Setup recursive mapping using entry 510 in the upper L2 table.
//
//   NOTE: The flag for a page table entry is the same as the flag for a page
//         entry to allow recursion.
  ldr     r0, =RECURSIVE_L2_OFFSET
  ldr     r1, =MMU_NORMAL_RW_FLAGS | MM_TYPE_PAGE_TABLE
  orr     r1, r10, r1
  str     r1, [r10, r0]

// Map the vectors into the kernel page tables.
  bl      layout_get_physical_exception_vectors_start
  mov     r1, r0
  mov     r0, r10
  bl      map_vectors

// Map the kernel area as RW normal memory.
//
//   TODO: The code should probably be separate from the stack and page tables
//         to prevent the code from being re-written.
  mov     r0, r9
  mov     r1, #0
  ldr     r2, =__virtual_start
  add     r3, r2, r5
  sub     r3, r3, #1
  ldr     r4, =MMU_NORMAL_RW_FLAGS | MM_TYPE_BLOCK
  push    {r4}
  bl      map_block
  pop     {r4}

// Map the DTB area as RO normal memory. Skip this if the DTB size is zero.
// Do not need to create an identity map. The kernel will switch to virtual
// addresses before the DTB is needed.
  cmp     r7, #0
  beq     1f

  mov     r0, r9
  mov     r1, r6
  ldr     r2, =__virtual_start
  add     r2, r2, r6
  add     r3, r2, r7
  sub     r3, r3, #1
  ldr     r4, =MMU_NORMAL_RO_FLAGS | MM_TYPE_BLOCK
  push    {r4}
  bl      map_block
  pop     {r4}

1:
// Clear the kernel identity page tables; save the start address.
  bl      layout_get_physical_id_pages_start
  mov     r9, r0
  mov     r1, #0
  ldr     r2, =__kernel_id_pages_size
  bl      memset

// Initialize the kernel identity page tables. The kernel identity pages are
// always going to handle more than 1 GiB since the kernel does not support a
// 1/3 split.
  mov     r0, r9
  mov     r1, #0
  bl      init_table
  mov     r9, r0            // Save the lower L2 table address
  mov     r10, r1           // Save the upper L2 table address

// Map the vectors into the kernel identity page tables.
  bl      layout_get_physical_exception_vectors_start
  mov     r1, r0
  mov     r0, r10
  bl      map_vectors

// Map the kernel area as RW normal memory. See above.
  mov     r0, r9
  mov     r1, #0
  mov     r2, #0
  add     r3, r2, r5
  sub     r3, r3, #1
  ldr     r4, =MMU_NORMAL_RW_FLAGS | MM_TYPE_BLOCK
  push    {r4}
  bl      map_block
  pop     {r4}

  pop     {r4, r5, r6, r7, r8, r9, r10}
  fn_exit
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Map the primary core's SVC, ABT, IRQ, and FIQ stacks.
///
/// # Parameters
///
/// * r0 - Virtual start address of the primary core's SVC stack.
/// * r1 - The virtual memory split.
///
/// # Description
///
/// The primary core's stacks will be mapped in the following order:
///
///     +---------------------------+ r0
///     | SVC Stack                 |
///     +---------------------------+
///     | / / / / / Guard / / / / / |
///     +---------------------------+
///     | ABT Stack                 |
///     +---------------------------+
///     | / / / / / Guard / / / / / |
///     +---------------------------+
///     | IRQ Stack                 |
///     +---------------------------+
///     | / / / / / Guard / / / / / |
///     +---------------------------+
///     | FIQ Stack                 |
///     +---------------------------+
///     | / / / / / Guard / / / / / |
///     +---------------------------+ r0 - 4 * (stack pages + 1) * page size
///
/// Upon return, the stack pointer will be updated to the provided start
/// address.
///
/// # Assumptions
///
/// Assumes the stack will not require multiple L3 tables.
///
/// Assumes the MMU is enabled and the identity tables are still configured.
///
/// Assumes the caller is in SVC.
///
/// Assumes that the stack is initially empty on entry.
.global mmu_setup_primary_core_stacks
mmu_setup_primary_core_stacks:
// r4 - Current table address
// r5 - Next table address
// r6 - Physical stack base
// r7 - Stack size
// r8 - Table index
// r9 - Page size
// r10 - Temp
  push    {r4, r5, r6, r7, r8, r9, r10}

  ldr     r9, =__page_size

// If using a 2/2 split, we will have a L1 and L2 table and we just need to
// skip the L1 table. If using a 3/1 split, we only have a L2 table. NOTE: the
// MMU is on, so we will get a virtual address for __kernel_pages_start. It will
// be linearly-mapped, however, so we can just subtract __virtual_start, then
// use physical addresses for both the table entries and writing to the table.
  ldr     r4, =__kernel_pages_start
  ldr     r10, =__virtual_start
  sub     r4, r4, r10       // L1 or L2 physical address
  cmp     r1, #3
  beq     1f                // If a 3/1 split, skip table increment
  add     r4, r4, r9        // Skip L1 table

1:
// Set up the L3 pointer.
  mov     r5, r4
  add     r5, r5, r9        // Skip existing L2
  add     r5, r5, r9        // Skip existing L3

// Get the stack size.
  ldr     r7, =__kernel_stack_pages
  mul     r7, r7, r9

// Preserve the virtual stack start in the frame pointer.
  mov     fp, r0

// Calculate the base of the FIQ stack.
  mov     r10, r7
  add     r10, r10, r9      // Size with guard page
  lsl     r10, r10, #2      // Four stacks
  sub     r0, r0, r10

// Get the L2 entry address.
  mov     r8, r0
  lsr     r8, r8, #L2_SHIFT
  ldr     r10, =0x1ff
  and     r8, r8, r10
  add     r4, r8, lsl #3

// Store the L3 pointer in the L2 table.
  mov     r10, r5
  orr     r10, r10, #MM_TYPE_PAGE_TABLE
  str     r10, [r4], #4
  mov     r10, #0
  str     r10, [r4], #4

// Set up the L3 pointer.
  mov     r4, r5

// Get the first L3 entry address.
  mov     r8, r0
  lsr     r8, r8, #L3_SHIFT
  ldr     r10, =0x1ff
  and     r8, r8, r10
  add     r4, r8, lsl #3

// Get the physical base of the FIQ stack.
  ldr     r6, =__kernel_svc_stack_start
  ldr     r10, =__virtual_start
  sub     r6, r6, r10
  sub     r6, r7, lsl #2

// Set up the page entries.
  mov     r10, #(MMU_NORMAL_RW_FLAGS | MM_TYPE_PAGE)
  orr     r6, r6, r10

// Outer loop for the four stacks.
  mov     r2, #0            // Zero each entry's high word
  mov     r3, #4

2:
// Inner loop over the stack pages.
  add     r4, r4, #8        // Skip guard page entry
  mov     r10, r7           // Stack size counter

3:
  str     r6, [r4], #4      // Store the entry low word
  str     r2, [r4], #4      // Store the entry high word
  add     r6, r6, r9        // Increment physical stack page

  sub     r10, r10, r9      // Subtract page size
  cmp     r10, #0
  bne     3b                // Loop back to add next entry

  sub     r3, r3, #1        // Decrement stack count
  cmp     r3, #0
  bne     2b                // Loop back to add next stack

// Pop the registers and manually restore the frame pointer to change the stack
// over to the correct pointer before returning.
  pop     {r4, r5, r6, r7, r8, r9, r10}
  mov     sp, fp
  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Set the MMU flags and enable the MMU.
///
/// # Description
///
///   NOTE: The function must be called with the link register set to the
///         VIRTUAL return address.
///
///   NOTE: This function will be called by the secondary cores before they have
///         stacks. This function MUST not modify callee-saved registers or call
///         other functions.
.global mmu_setup_and_enable
mmu_setup_and_enable:
  mov     r2, lr

  bl      setup_ttbr

  ldr     r0, =__vmsplit
  bl      make_ttbcr_value
  mcr     p15, 0, r0, c2, c0, 2

  ldr     r0, =DACR_VALUE
  mcr     p15, 0, r0, c3, c0, 0

  isb
  mrc     p15, 0, r0, c1, c0, 0
  ldr     r1, =SCTLR_FLAGS
  orr     r0, r0, r1
  mcr     p15, 0, r0, c1, c0, 0
  isb

  mov     pc, r2


///-----------------------------------------------------------------------------
///
/// Cleanup the translation table registers after enabling the MMU.
///
/// # Description
///
/// Zeros out TTBR0 leaving TTBR1 with the kernel pages.
///
///   NOTE: This function will be called by the secondary cores before they have
///         stacks. This function MUST not modify callee-saved registers or call
///         other functions.
.global mmu_cleanup_ttbr
mmu_cleanup_ttbr:
// Zero out TTBR0.
  mov     r0, #0
  mov     r1, #0
  mcrr    p15, 0, r0, r1, c2

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Update an entry in a translation table and selectively invalidate the caches
/// by virtual address.
///
/// # Parameters
///
/// * r0 - The descriptor virtual address.
/// * r1 - The virtual address being remapped.
/// * r2 - The low word of the large descriptor.
/// * r3 - The high word of the large descriptor.
///
/// # Description
///
/// The unified TLB and Branch Predictors will be invalidated for the virtual
/// address being remapped. This function is appropriate for precision changes
/// to the translation tables, e.g. swapping a task's local mapping table into
/// the kernel's address space or mapping a page locally.
.global mmu_update_table_entry_local
mmu_update_table_entry_local:
// Make the new entry and ensure visibility. Cleaning the cache line when the
// table is write-back cacheable memory is not required with an ARMv7
// implementation that includes multiprocessing extensions.
  str     r2, [r0], #4
  str     r3, [r0], #4
  dsb

// Invalidate unified TLB and Branch Predictor by virtual address (See TLBIMVA
// in B3.18.7 and BPIMVA in B3.18.6), and ensure completion.
  mcr     p15, 0, r1, c8, c7, 1
  mcr     p15, 0, r1, c7, c5, 7
  dsb
  isb

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Section-align a memory block.
///
/// # Parameters
///
/// * r0 - The base address of the block.
/// * r1 - The size of the block.
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
  add     r2, r0, r1

// Calculate the section mask.
  ldr     r1, =SECTION_SIZE
  sub     r1, r1, #1

// Section align the end address.
  add     r2, r2, r1
  mvn     r1, r1
  and     r2, r2, r1

// Section align the base address.
  and     r0, r0, r1

// Calculate the new size.
  sub     r1, r2, r0

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Map the exception vector page.
///
/// # Parameters
///
/// * r0 - The base address of the L2 table for the top 1 GiB.
/// * r1 - The base address of the exception vectors.
///
/// # Description
///
/// Creates a L3 table with entries for the exception vector pages.
///
/// # Assumptions
///
/// Assumes that the page after the L2 table is free.
map_vectors:
// Get the address for the new L3 table.
  ldr     r2, =__page_size
  add     r2, r2, r0
  orr     r2, r2, #MM_TYPE_PAGE_TABLE

// Entry 511 in the L2 table covers the last 2 MiB of the address space.
  ldr     r3, =VECTORS_L2_OFFSET
  add     r3, r0, r3
  str     r2, [r3], #4
  mov     r2, #0
  str     r2, [r3], #4

// r3 now points to the L3 table and entry 496 covers the page at 0xffff_0000.
  mov     r0, r3
  ldr     r3, =VECTORS_L3_OFFSET
  add     r3, r0, r3

// Make the descriptor for the vectors.
  ldr     r2, =MMU_NORMAL_RO_FLAGS | MM_TYPE_PAGE
  orr     r1, r1, r2
  mov     r2, #0
  str     r1, [r3], #4
  str     r2, [r3], #4

// Make the descriptor for the stubs.
  ldr     r2, =__page_size
  add     r1, r1, r2
  mov     r2, #0
  str     r1, [r3], #4
  str     r2, [r3], #4

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Initialize the indirect memory attribute registers.
init_mair:
  ldr     r0, =MAIR0_VALUE
  mcr     p15, 0, r0, c10, c2, 0

  ldr     r0, =MAIR1_VALUE
  mcr     p15, 0, r0, c10, c2, 1

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Initialize the L1 table.
///
/// # Parameters
///
/// * r0 - The base address of the L1 table.
/// * r1 - The base address of the virtual address space.
///
/// # Description
///
///   NOTE: When using a 2/2 split, the input address size is 31 bits, so only
///         bit 30 is used to index the Level 1 table. This means both Level 1
///         tables use entries 0 and 1.
///
/// # Assumptions
///
/// Assumes that a 2/2 virtual memory split is being used and creates entries
/// for two L2 tables that cover the 2 GiB of the address space.
///
/// # Returns
///
/// The addresses of the lower and upper L2 tables.
init_table:
// Get the entry address.
  lsr     r2, r1, #L1_SHIFT // Top two bits are the index
  and     r2, r2, #1        // See note in function header
  lsl     r2, r2, #3        // 8 bytes per entry
  add     r2, r0, r2        // Add the base address

// Create the table entry for the lower 1 GiB. The descriptor has to be split
// between two 32-bit registers. r3 will be the lower 32-bits and the upper
// 32-bits will be 0 since our physical address does not need the extra 8 bits
// and we do not need to set any of the upper attributes.
  ldr     r3, =__page_size
  add     r0, r0, r3        // r0 is now the lower L2 table address
  mov     r3, r0
  orr     r3, r3, #MM_TYPE_PAGE_TABLE

// Store the entry in the table.
  str     r3, [r2], #4      // Lower 32-bits
  mov     r3, #0
  str     r3, [r2], #4      // Upper 32-bits

// Create the table entry for the upper 1 GiB. See above.
  ldr     r3, =__page_size
  add     r1, r0, r3        // r1 is now the upper L2 table address
  mov     r3, r1
  orr     r3, r3, #MM_TYPE_PAGE_TABLE
  
// Store the entry in the table.
  str     r3, [r2], #4      // Lower 32-bits
  mov     r3, #0
  str     r3, [r2], #4      // Upper 32-bits

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Map a block of 2 MiB sections to the L2 translation table.
///
/// # Parameters
///
/// * r0 - The base address of the L2 table.
/// * r1 - The base physical address.
/// * r2 - The base virtual address.
/// * r3 - The last virtual address.
/// * stack - The entry flags.
map_block:
  push    {r4, r5}

  ldr     r4, [sp, #8]
  mov     r5, #L2_TABLE_ENTRY_CNT - 1

  lsr     r2, r2, #SECTION_SHIFT
  and     r2, r2, r5
  lsr     r3, r3, #SECTION_SHIFT
  and     r3, r3, r5
  lsr     r1, r1, #SECTION_SHIFT
  orr     r1, r4, r1, lsl #SECTION_SHIFT
1:
// Same as `init_table`. The table entries are 64-bit, but the 20-bit pointers
// in r1 are complete and there are no upper attributes to set. The upper 32-
// bits of the descriptor can be left as zero. Store by shifting the index left
// 3 bits.
  str     r1, [r0, r2, lsl #3]
  add     r2, r2, #1
  add     r1, r1, #SECTION_SIZE
  cmp     r2, r3
  bls     1b

  pop     {r4, r5}

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Setup the TTBCR flags for the MMU.
///
/// * r0 - The virtual memory split.
///
/// # Returns
///
/// Configures the bootstrap TTBCR value. If the split value is 3, a 3/1 split
/// is used. Otherwise, a 2/2 split is used.
make_ttbcr_value:
  ldr     r1, =TTBCR_VALUE

  cmp     r0, #3
  bne     1f

  orr     r1, #TTBCR_T1SZ_3
  b       2f

1:
  orr     r1, #TTBCR_T1SZ_2

2:
  mov     r0, r1

  mov     pc, lr


///-----------------------------------------------------------------------------
///
/// Setup the translation table registers before enabling the MMU.
///
/// # Description
///
/// Sets up TTBR0 with the identity tables and TTBR1 with the kernel tables.
setup_ttbr:
  fn_entry

// Set TTBR1 to the kernel pages.
  bl      layout_get_physical_pages_start
  mov     r1, #0
  mcrr    p15, 1, r0, r1, c2

// Set TTBR0 to the identity pages.
  bl      layout_get_physical_id_pages_start
  mov     r1, #0
  mcrr    p15, 0, r0, r1, c2

  fn_exit
  mov     pc, lr
