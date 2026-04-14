# THE PROPELLER KERNEL - PART 6

## Introduction

[Virtual Memory](https://en.wikipedia.org/wiki/Virtual_memory) is a technique that provides tasks with their own private address spaces, thus partitioning tasks from each other and the kernel and abstracting memory for user tasks.

At a very high level, modern processors provide [Memory Management Units (MMUs)](https://en.wikipedia.org/wiki/Memory_management_unit) that serve as intermediaries between code and physical memory when enabled. While the MMU is on, all memory addresses are treated as "virtual". When an instruction attempts to read or write memory, the MMU transparently translates the address from virtual to physical using a configuration provided by the kernel. If the translation fails, the MMU raises an exception.

If the kernel has its own configuration and every user tasks has its own configuration, then they are effectively partitioned from each other by the MMU.

Not only does this achieve partitioning, but it allows the kernel to place and move data in physical memory without user tasks needing to know the details.

If a user task allocates a large block of memory, for example, the kernel can allocate physical memory in what ever way is convenient and provide a configuration that makes the block appear contiguous to the user task.

If the kernel needs to give physical memory to another task, it can [swap](https://en.wikipedia.org/wiki/Memory_paging#Page_faults) the task's data out of physical memory. When swapping the task's data back into physical memory, it may not end up in the same place. However, the user task will never know.

A global MMU configuration called the "split" defines the overall Virtual Memory address layout. We briefly touched on Virtual Memory layouts in [Part 3](https://slakpi.github.io/Propeller-Kernel/part_3.html).

For ARM, a general purpose 32-bit virtual address space will look like:

         2:2 Split                             3:1 Split
         ---------                             ---------

    +-----------------+ 0xffff_ffff       +-----------------+ 0xffff_ffff
    |                 |                   |                 |
    |                 |                   | Kernel Segment  | 1 GiB
    | Kernel Segment  | 2 GiB             |                 |
    |                 |                   +-----------------+ 0xc000_0000
    |                 |                   |                 |
    +-----------------+ 0x8000_0000       |                 |
    |                 |                   |                 |
    |                 |                   | User Segment    | 3 GiB
    | User Segment    | 2 GiB             |                 |
    |                 |                   |                 |
    |                 |                   |                 |
    +-----------------+ 0x0000_0000       +-----------------+ 0x0000_0000

For AArch64, a typical general purpose 64-bit virtual address space looks like:

    +-----------------+ 0xffff_ffff_ffff_ffff
    |                 |
    | Kernel Segment  | 256 TiB
    |                 |
    +-----------------+ 0xffff_0000_0000_0000
    | / / / / / / / / |
    | / / / / / / / / |
    | / / / / / / / / | 16,776,704 TiB (Unused)
    | / / / / / / / / |
    | / / / / / / / / |
    +-----------------+ 0x0000_ffff_ffff_ffff
    |                 |
    | User Segment    | 256 TiB
    |                 |
    +-----------------+ 0x0000_0000_0000_0000

> NOTE: Putting the kernel at the top of the address space is just a convention. It is equally valid to put the kernel at the bottom. In fact, ARM provides a 1:3 split option that would be the equivalent of the 3:1 split but with the kernel starting at 0x0 and user memory starting at 0x4000_0000.

The "split" in the examples above is the number of most-significant bits that need to be 1 to distinguish a virtual user address from a virtual kernel address.

In the 32-bit 2:2 split example, an address is a virtual kernel address if bit 31 is 1. In the 32-bit 3:1 split example, an address is a virtual kernel address if bits 31:30 are 1.

In the 64-bit example, an address is a virtual kernel address if bits 63:48 are 1. An address is a virtual user address only if bits 63:48 are 0. If bits 63:48 are any other combination of 0's and 1's, the address is invalid.

Commonly, processors maintain two active configurations, one for the kernel and one for the currently running user task, and the MMU chooses the appropriate configuration based on the upper bits of the virtual address being translated.

Whenever the kernel preempts a running task and activates another task, it replaces the user configuration with the new task's configuration. Thus every user task sees the same *virtual* address space, but the kernel chooses how those virtual addresses translate to *physical* addresses.

> NOTE: This is the ***general purpose*** way of doing things. Embedded systems that know their workload and memory requirements at compile time can greatly simplify the kernel by not providing dynamic memory allocation and using a fixed partitioning of the address space between each user task. This setup keeps the protection provided by the MMU, but operates in a deterministic way appropriate for specialized systems.

## Translation Tables

So, what are these "configurations"? Translation tables! A translation table maps virtual addresses to physical addresses. While a user task may see the entirety of its own address space, it can only access the addresses for which the kernel has provided translations. Accessing any other virtual address will cause an exception.

> NOTE: An exception here is not necessarily a "bad" thing. It could be that the user task or the kernel is attempting to access memory it is not allowed to access. But, in the case of user task, it could be that the task is accessing memory that has been swapped out and needs to be swapped back.

If a translation table had an entry for every word on a 32-bit system, the table would consume 1 GiB of memory. So, at their most granular level, tables map pages. If each entry in the table represents a 4 KiB page, the table now only consumes 1 MiB of memory.

Can we do better? If, instead of a single monolithic table, the MMU translates in stages, a 32-bit system could use a 16 KiB table with 4,096 entries that map 1 MiB sections. The entries in this table could either map the entire 1 MiB section into virtual memory or point to a 1 KiB second-level table with 256 entries that map each 4 KiB page within the 1 MiB section into virtual memory.

Virtual addresses spaces are typically sparse, so multiple levels of translation are far more space efficient if slightly slower.

### ARM

The two-level scheme discussed above is how 32-bit ARM processors without [Large Physical Address Extensions](https://developer.arm.com/documentation/den0013/0400/Virtualization/Large-Physical-Address-Extensions?lang=en) operate. With LPAE, ARMv7-A extends this two-level translation scheme into three levels.

	Level 1       ->  Level 2       -> Level 3
	4 Entries         512 Entries      512 Entries
	Covers 4 GiB      Covers 1 GiB     Covers 2 MiB

Instead of a 16 KiB Level 1 table and 1 KiB Level 2 tables, all tables are 4 KiB and  each table has 512 64-bit entries (only the first four are used at Level 1).

An entry at Level 2 can either map a full 2 MiB section into virtual memory, or point to a Level 3 table that maps 4 KiB pages into virtual memory. When mapping a large contiguous chunk of memory, it can be more space and time efficient to skip Level 3 translation by mapping the chunk with 2 MiB sections.

LPAE is also required for a 3:1 split. Propeller will halt if LPAE is not present.

Another trick LPAE-enabled ARM processors use is skipping Level 1 if the address space only uses the lower or upper 1 GiB. In the case of the 3:1 split, an address starting with 0xc000_0000 can only ever use the fourth entry in a Level 1 table, so the processor just skips Level 1 and assumes translation starts with a Level 2 table.

### AArch64

AArch64 uses four levels of translation to cover the much larger address space.

	Level 1  ->  Level 2  ->  Level 3  ->  Level 4
	Covers       Covers       Covers       Covers
	256 TiB      512 GiB      1 GiB        2 MiB

Like ARM with LPAE, all tables are 4 KiB with 512 64-bit entries.

AArch64 allows mapping 1 GiB and 2 MiB sections at Levels 2 and 3 respectively.

### Virtual Addresses

Virtual addresses are essentially composed of indices into these hierarchical tables.

32-bit addresses break down as follows:

	2 MiB section virtual address layout:
	+----+--------+--------------------+
	| L1 |   L2   |       Offset       |
	+----+--------+--------------------+
	31  30       21                    0

	4 KiB page virtual address layout:
	+----+--------+--------+-----------+
	| L1 |   L2   |   L3   |  Offset   |
	+----+--------+--------+-----------+
	31  30       21       12           0

Bits 31:30 index the Level 1 table.

Bits 29:21 index the Level 2 table.

If the Level 2 table maps to a 2 MiB section, the MMU treats bits 20:0 as an offset into the section. Otherwise, it uses bits 20:12 to index the Level 3 table and bits 11:0 as an offset into the page.

64-bit virtual addresses have a similar break down:

	2 MiB section virtual address layout:
	+---------------+--------+--------+--------+--------------------+
	| / / / / / / / |   L1   |   L2   |   L3   |       Offset       |
	+---------------+--------+--------+--------+--------------------+
	63             48       39       30       21                    0

	4 KiB page virtual address layout:
	+---------------+--------+--------+--------+--------+-----------+
	| / / / / / / / |   L1   |   L2   |   L3   |   L4   |  Offset   |
	+---------------+--------+--------+--------+--------+-----------+
	63             48       39       30       21       12           0

## Propeller's Initial Translation Tables

So far, we have been relying on position independent code and relative offsets to work around the fact that our linker script is using virtual addresses, but the MMU has not been configured and enabled. At a minimum, we need to enable the MMU and configure the translation tables to map the kernel into virtual memory so that absolute addressing works.

Once the MMU is enabled, the kernel is not going to be able to access the DeviceTree blob with a physical address, so we are going to need to map the DeviceTree into virtual memory as well.

That is the extent of what the start code can accomplish without parsing the DeviceTree to determine how much physical memory is available and where.

We have three main tasks ahead of us:

1. Find somewhere to store some initial translation tables.
2. Configure the translation tables to map the kernel and DeviceTree into virtual memory.
3. Configure the MMU's global settings and enable it.

### Step 1: Where do we put the translation tables?

We can solve this problem the same way we solved the initial stack problem in Part 5. We can just reserve space in the kernel image for some initial translation tables.

	+-----------------+
	| .data.tables    |
	|.................|
	| / / / / / / / / | (page alignment)
	|.................|
	| .data.stack     |
	|.................|
	| / / / / / / / / | (page alignment)
	|.................|
	| .bss            |
	|.................|
	| / / / / / / / / | (page alignment)
	|.................|
	| .data           |
	|.................|
	| / / / / / / / / | (page alignment)
	|.................|
	| .rodata         |
	|.................|
	| / / / / / / / / | (page alignment)
	|.................|
	| .text           |
	|                 |
	| .text.boot      |
	+-----------------+ base address

Let's update the linker script reserve space for 16 translation tables.

```
__page_size = 4096;
__kernel_stack_pages = 2;
__kernel_page_tables = 16;

...

SECTIONS
{
  ...

  .data.tables : ALIGN(__page_size)
  {
    __kernel_tables_start = .;
    . += (__kernel_page_tables * __page_size);
    __kernel_tables_end = .;
  }
}
```

#### Step 1A: Is that all?

Consider everything we have done up to this point. Remember the discussion about position independent code being necessary so that the Program Counter can add offsets the ***physical*** address it is using?

Now consider what we are about to do. We are about to provide the MMU with translation tables for the kernel's virtual address space and then turn the MMU on.

What is going to happen the moment after we turn the MMU on when the Program Counter tries to fetch the next instruction using a ***physical*** address? Well, quite simply: we are going to get an exception.

We are far from having any user tasks running, so this problem is easily solved by configuring the MMU with one set of tables for the kernel's address space and a second set of identity tables that just map physical addresses back to the same physical address. After turning the MMU on, we add an instruction that performs a jump to a virtual kernel address and now the Program Counter is using addresses in the kernel's virtual address space. Voila.

Let's update the kernel image to add space for these identity tables. Also, let's add some markers to let us know what the virtual start address is (`__virtual_start`), where the kernel image starts (`__kernel_start`), the size of the kernel's text, rodata, data, and BSS sections (`__kernel_size`), and where the kernel image ends (`__kernel_end`).

```
__page_size = 4096;
__kernel_stack_pages = 2;
__kernel_page_tables = 16;
__virtual_start = 0xffff000000000000;

...

SECTIONS
{
  .text :
  {
    __kernel_start = .;

    KEEP(*(.text.boot))
    *(.text)
  }
  
  ...

  __kernel_size = . - __kernel_start;

  .data.id_tables : ALIGN(__page_size)
  {
    __kernel_id_tables_start = .;
    . += (__kernel_page_tables * __page_size);
    __kernel_id_tables_end = .;
  }

  __kernel_id_tables_size = __kernel_id_tables_end - __kernel_id_pages_start;

  .data.tables : ALIGN(__page_size)
  {
    __kernel_tables_start = .;
    . += (__kernel_page_tables * __page_size);
    __kernel_tables_end = .;
  }
  
  __kernel_tables_size = __kernel_tables_end - __kernel_tables_start;

  __kernel_end = __kernel_pages_end;
}
```

### Step 2: How do we configure the translation tables?

We are going to map physical memory into the kernel's virtual address space, so the first thing we need to know is what we are mapping. We are going to linearly map the kernel image and DeviceTree into the kernel's address space by simply adding the kernel's virtual base address. The identity tables will just map the physical addresses back to themselves.

          Identity              Virtual
          Map                   Map

       +---------------+     +---------------+ VS + PE
       | / / / / / / / |     | DTB           |
       | / / / / / / / |     +---------------+ VS + PS
       | / / / / / / / |     | / / / / / / / |
       | / / / / / / / |     | / / / / / / / |
    KE +---------------+     +---------------+ VS + KE
       |               |     |               |
       | Kernel Image  |     | Kernel Image  |
       |               |     |               |
    KS +---------------+     +---------------+ VS + KS
       | / / / / / / / |     | / / / / / / / |   
     0 +---------------+     +---------------+ VS

| Abbreviation | Description                              |
|:-------------|:-----------------------------------------|
| `VS`         | `__virtual_start`                         |
| `KS`         | `__kernel_start`                          |
| `KE`         | Section-aligned `__kernel_end`            |
| `PS`         | Blob pointer provided by the boot loader |
| `PE`         | Section-aligned blob size                |

> NOTE: We are section-aligning the size of the kernel and the DeviceTree. We are only going to use 2 MiB sections for the initial translation tables. So, if the kernel is 2.9 MiB, we will map 4 MiB of address space.

> NOTE: There is no reason to map the DeviceTree in the identity tables. We will never access it through the physical address once the MMU is enabled.

If all of that makes sense, one thing you may be asking is: How do we know the size of the DeviceTree. If you recall from Part 2, the boot loader provides the physical address of the start of the DeviceTree (or ATAGs), but tells us nothing about its size.

#### Step 2A: Quick diversion into DeviceTrees

Let me introduce you to the [Flattened DeviceTree format](https://devicetree-specification.readthedocs.io/en/latest/chapter5-flattened-format.html). [Section 5.2](https://devicetree-specification.readthedocs.io/en/latest/chapter5-flattened-format.html#header) discusses the format of the DTB header. The first two 32-bit words in the DTB header are the "magic" bytes (0xd00d_feed) followed by the total size of the DTB.

[dtb.s](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/dtb.s) in the AArch64 start module has a small utility function named `dtb_quick_check`. This function checks the provided DTB address for the magic bytes, then either returns the size of DTB if the boot loader provided a DTB or returns 0 if the boot loader provided something other than a DTB.

Picking up where Part 5 left off, the first thing the start code does after zeroing the BSS section is check the DTB size at [start.s:224](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L224).

```assembly
// Check if the blob is a DTB. The kernel does not support ATAGs.
  mov     x0, x19
  bl      dtb_quick_check
  cbz     x0, cpu_halt
```

The MMU is still off at this point, so using DTB's physical address is fine.

We now have all of the variables above needed to generate the translation tables.

#### Step 2B: Great, now what actually goes in the translation tables?

As described above, AArch64 has four levels of translation tables. We are going to assume that the kernel and DTB are fully contained within the first 1 GiB of memory. Furthermore, we are only going to map 2 MiB sections. This means we only need three tables:

	     Level 1                Level 2                Level 3
	     256 TiB                512 GiB                1 GiB
	    +----------+           +----------+           +----------+
	  0 |       --------->   0 |       --------->   0 | Mapping  |
	   ...        ...         ...        ...         ...        ...
	511 |          |       511 |          |       511 | Entries  |
	    +----------+           +----------+           +----------+

We will zero all three tables. Entry 0 in the Level 1 table will point to the Level 2 table. Entry 0 in the Level 2 table will point to the Level 3 table. And the Level 3 table will have the 2 MiB section entries.

For detailed information about the table entries, refer to the AArch64 Reference Manual [Section D8.3.1](https://developer.arm.com/documentation/ddi0487/mb/-Part-D-The-AArch64-System-Level-Architecture/-Chapter-D8-The-AArch64-Virtual-Memory-System-Architecture/-D8-3-Translation-table-descriptor-formats/-D8-3-1-VMSAv8-64-descriptor-formats?lang=en) for 48-bit output addresses with a 4 KiB granule (page size). Refer to the ARMv7-A Reference Manual Section B3.6.1 for long descriptor formats with a 4 KiB granule.

Read those carefully. This is going to be your first foray into individual bits meaning the difference between everything working and nothing working.

At a high level, the AArch64 pointer entry format that we will use in the Level 1 and 2 tables is:

| Bit(s) | Value / Meaning                          |
|:-------|:-----------------------------------------|
| 63:59  | Upper attributes (zero for our purposes) |
| 58:48  | b0                                       |
| 47:12  | Bits 47:12 of the physical table address |
| 11:2   | b0                                       |
| 1:0    | b11                                      |

> NOTE: Bits 63:48 of the physical table address are ignored because of the address space split. Bits 11:0 of the address are assumed zero because the table must be page-aligned. Only bits 47:12 of the physical address are significant.

The 2 MiB section entry format we will use in the Level 3 table is:

| Bit(s) | Value / Meaning                              |
|:-------|:---------------------------------------------|
| 63:50  | Upper attributes (zero for our purposes)     |
| 49:48  | b0                                           |
| 47:21  | Bits 47:21 of the section's physical address |
| 20:17  | b0                                           |
| 16     | b0 (not using nT)                            |
| 15:12  | b0 (not using large physical addresses)      |
| 11     | b0 (single privilege level)                  |
| 10     | b1 (accessed, allow TLB to cache entry)      |
| 9:8    | b0 (not using large physical addresses)      |
| 7:6    | b00 (read/write) or b01 (read only)          |
| 5:2    | b0 (memory attribute index)                  |
| 1:0    | b01                                          |

> NOTE: Like table pointers, bits 63:48 of the section's physical address are ignored. Bits 20:0 are assumed zero because the section is aligned to 2 MiB. Only bits 47:21 of the section's physical address are significant.

> NOTE: We will talk more about the memory attribute index in bits 5:2 when discussing the global MMU configuration. For now, just understand that we will declare the properties of normal (versus device) memory at index 0 in [MAIR_EL1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-mair_el1), thus we are telling the MMU that these sections are normal memory.

#### Step 2C: Building the tables

[mm.s](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s) in the AArch64 start module has all of the code used to configure the MMU, build the translation tables, and enable the MMU. For this section, we will focus just on the code to build the tables. The bulk of that code is in [`mmu_create_kernel_page_tables`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L108).

The first part of the function calculates the section-aligned sizes, then clears all of the reserved space for the translation tables. Any entry with bit 0 set to 0 is invalid.

[`init_tables`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L417) is used to initialize the Level 1 and Level 2 tables for both the kernel and identity tables. This function uses [`create_table_entry`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L443) to add the Level 1 and Level 2 pointer entries, then returns the address of the corresponding Level 3 table.

Finally, [`map_block`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L476) is used to map the 2 MiB sections of the kernel in both the kernel and identity tables. This memory is mapped as read/write since our initial stack is part of the image and we are going to edit the translation tables later. `map_block` is used one last time to map the DeviceTree into the kernel tables as read-only memory.

> NOTE: At some point, it might make sense to section align the initial stack and translation tables so that the actual kernel code can be mapped as read-only while the stack and tables can be mapped as read/write.

[start.s:229](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L229) calls `mmu_create_kernel_page_tables` with the base address and the size of the DeviceTree. After the call to `dtb_quick_check`, the size of the DeviceTree is in `x0`.

```assembly
// Create the bootstrap kernel page tables.
  mov     x1, x0            // DTB blob size to x1
  mov     x0, x19           // DTB blob address to x0
  bl      mmu_create_kernel_page_tables
```

That's all for setting up the translation tables. It's a fair amount of code and bit fiddling, but conceptually pretty simple.

### Step 3: Configure the MMU's global settings and enable it

[mm.s:38](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L38) breaks down the bits we are going to set in the Translation Control Register ([TCR_EL1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-tcr_el1)).

| Bit(s) | Value / Meaning                                 |
|:-------|:------------------------------------------------|
| 31:30  | b10 (kernel granule size - 4 KiB)               |
| 27:26  | b0 (outer non-cacheable for kernel table walks) |
| 25:24  | b01 (inner cacheability for kernel table walks) |
| 21:16  | b10000 (kernel region size - 2^[64 - m])        |
| 15:14  | b00 (user granule size - 4 KiB)                 |
| 11:10  | b0 (outer non-cacheable for user table walks)   |
| 9:8    | b01 (inner cacheability for user table walks)   |
| 5:0    | b10000 (user region size - 2^[64 - n])          |

System on Chips has a good explanation of [Inner and Outer](https://www.systemonchips.com/armv8-multi-cluster-cache-coherency-and-inner-shareable-memory-configuration/) domains. We are not concerned with multi-cluster systems, so we are just telling the MMU not to cache entries across inner sharing domains.

[mm.s:69](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L69) breaks down the bits we are going to set in the Memory Attribute Indirection Register ([MAIR_EL1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-mair_el1)).

Rather than setting the same memory attribute bits on every translation table entry, `MAIR_EL1` allows creating up to 8 different memory attribute sets that we can reference by index in the translation table entries. Right now, we only really need to configure attribute sets for normal and device memory.

| Bit(s) | Value / Meaning                                 |
|:-------|:------------------------------------------------|
| 63:16  | b0                                              |
| 15:8   | b0 (device memory, no caching)                  |
| 7:0    | b11111111 (normal memory, caching)              |

[`mmu_setup_and_enable`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L328) is responsible for setting TCR_EL1 and MAIR_EL1, setting up the translation table pointers, and enabling the MMU.

[TTBR1_EL1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-ttbr1_el1) is the translation table pointer for the virtual address space starting at 0xffff_0000_0000_0000. This register will be set to the physical address of the kernel's translation table.

[TTBR0_EL1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-ttbr0_el1) is the translation table pointer for the virtual address space starting at 0x0. Typically, this will be set to the physical address of the running task's translation table. For now, we will set it to the identity table.

### Jumping to Virtual Addressing

[start.s:234](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L234) calls `mmu_setup_and_enable` and performs the jump to virtual addressing via the Link Register.

```assembly
// Enable the MMU.
//
//   NOTE: Manually set the link register to the virtual return address when
//         calling `mmu_setup_and_enable`. Do not use branch-and-link.
  adrp    x0, __kernel_id_pages_start
  adrp    x1, __kernel_pages_start
  ldr     lr, =primary_core_begin_virt_addressing
  b       mmu_setup_and_enable

primary_core_begin_virt_addressing:
```

The code passes the physical addresses for the Level 1 identity and kernel tables to `mmu_setup_and_enable` and manually sets the Link Register to the absolute ***virtual*** address of the `primary_core_begin_virt_addressing` label. This allows the return from `mmu_setup_and_enable` to jump the Program Counter to virtual addressing.

And there we are. We've gone virtual!

The easy way to test this with GDB is to set a breakpoint at `primary_core_begin_vert_addressing`. If the MMU is set up correctly and enabled, GDB will break at the virtual address of the label.

We still have some cleanup work to do, but I think we've done enough for Part 6.

-----
[Part 7](https://slakpi.github.io/Propeller-Kernel/part_7.html)
