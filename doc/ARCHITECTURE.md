# PROPELLER KERNEL ARCHITECTURE

* [`start` Module](#arch-start-module)
* [`arch` Module](#arch-module)
  * [Module Interface](#arch-module-iface)
  * [ARM](#arm-arch-impl)
  * [AArch64](#aarch64-arch-impl)
* [`mm` Module](#mm-module)
  * [Pager](#pager)
  * [Page Database](#page-directory)
  * [Buddy Allocator](#buddy-allocator)
* [`support` Module](#support-module)
* [`sync` Module](#sync-module)

## `start` Module {#arch-start-module}

The `start` module contains the basic architecture assembly code to lay out the kernel image, set up the initial page tables, setup exception handlers, etc. The `start` module also contains any code that has to be written in assembly. For example, code to get the current core identifier or set the current task pointer. These are written directly in assembly rather than Rust inline assembly to simplify debugging. The `arch` module provides Rust wrappers for any utilities written in assembly.

The `start` module does not have a defined interface. Each architecture may implement its `start` module as needed to implement the `arch` interface.

## `arch` Module {#arch-module}

The `arch` module is an *interface* to architecture-specific Rust code. The module automatically includes the correct architecture code and exports it as the `arch` module.

### Module Interface {#arch-module-iface}

Each architecture supported by Propeller must implement the following public interface.

#### `fn init( config: usize )`

Performs single-threaded, architecture-specific kernel initialization. Typically, this will involve determining the amount of physical memory, setting up kernel page tables, setting up page allocators, etc.

#### `fn init_multi_core()`

Performs multithreaded initialization. Any secondary cores will be running with interrupts disabled when this function returns. This must be called after `init()`.

#### `fn get_page_size() -> usize`

Retrieves the page size.

#### `fn get_page_shift() -> usize`

Retrieves the number of bits to shift an address right to calculate a physical Page Frame Number (PFN).

#### `fn get_page_mask() -> usize`

Retrieves the address mask for a page offset.

#### `fn get_section_size() -> usize`

Retrieves the section size.

#### `fn get_section_shift() -> usize`

Retrieves the number of bits to shift an address right to calculate a physical section frame number.

#### `fn get_section_mask() -> usize`

Retries the address mask for a section offset.

#### `fn get_page_table_entry_size() -> usize`

Retrieves the size of a page table entry.

#### `fn get_page_table_entry_shift() -> usize`

Retrieves the size of bits to shift an offset right to calculate a page table index.

#### `fn get_kernel_base() -> usize`

Retrieves the kernel's physical base address.

#### `fn get_max_physical_address() -> usize`

Retrieves the maximum physical address. The maximum physical address is just the bitwise negation of the kernel's base physical address. For example, with 32-bit build and a 3/1 split, the kernel starts at 0xc000_0000 and the maximum physical address is 0x3fff_ffff. With a 64-bit build and the kernel starting at 0xffff_0000_0000_0000, the maximum physical address is 0x0000_ffff_ffff_ffff.

#### `fn get_kernel_virtual_base() -> usize`

Retrieves the kernel segment virtual base address.

#### `fn get_device_tree() -> &'static device_tree::DeviceTree`

Retrieves a reference to the system device tree.

#### `fn get_current_core_index() -> usize`

Convenience interface to get the sequential index of the current core from the system device tree.

#### `fn get_page_database_virtual_base() -> usize`

Retrieves the virtual address of the page database.

#### `fn get_page_database_size() -> usize`

Retrieves the size of the virtual area reserved for the page database in bytes.

#### `fn spin_lock( lock_addr: usize )`

Low-level spin lock on the specified address.

#### `fn try_spin_lock( lock_addr: usize ) -> bool`

Attempt a low-level spin lock on the specified address.

#### `fn spin_unlock( lock_addr: usize )`

Low-level spin lock release on the specified address.

#### `fn debug_print( args: fmt::Arguments )`

Implements architecture-dependent debug output. For example, Propeller currently uses the ARM UART to send debug messages.

### ARM Implementation {#arm-arch-impl}

#### Page size

`__page_size` is a compile-time constant provided by the linker script that specifies the size of a page. Propeller will currently panic if the page size is not 4 KiB.

#### Kernel Image Layout

    +----------------------+ __kernel_start / __text_start
    | .text                |
    +----------------------+ __rodata_start
    | .rodata              |
    +----------------------+ __data_start
    | .data                |
    +----------------------+ __bss_start
    | .bss                 |
    +----------------------+ __kernel_svc_stack_end
    |                      |
    |                      | __kernel_abt_stack_end
    |                      |
    | .data.stacks         | __kernel_irq_stack_end
    |                      |
    |                      | __kernel_fiq_stack_end
    |                      |
    +----------------------+ __kernel_stack_list
    | .data.stack_pointers |
    +----------------------+ __kernel_exception_vectors_start
    | .text.vectors        |
    +----------------------+ __kernel_exception_stubs_start
    | .text.stubs          |
    +----------------------+ __kernel_id_pages_start
    | .data.id_pages       |
    +----------------------+ __kernel_pages_start
    | .data.pages          |
    +----------------------+ __kernel_end

The base of the `.text` segment is specified by the compile-time constant `__kernel_start` provided by the build system.

`.data.stacks` is the primary core's interrupt service routine (ISR) stack. Refer to `SP_irq` and `SP_svc`. 

`.data.stacks` is an area reserved for `SP_svc`, `SP_abt`, `SP_irq`, and `SP_fiq`. `__kernel_stack_pages` is a compile-time constant provided by the linker script that specifies the size of the stacks in pages. During the single-threaded setup phase, the primary core uses `SP_svc` as its general purpose stack.

`.data.stack_pointers` is the ISR stack pointer table for secondary cores. During the single-threaded setup phase, the primary core will allocate pages for secondary core ISR stacks and place pointers to the tops of those stacks in this table. The secondary cores will index this table to locate their stacks when they are released.

The stack pointer table is a single page of 1024 4-byte pointer entries. 1024 entries is sufficient for the 256 core maximum on ARM nodes. See [Multi-Core Initialization](#arm-multi-core-init).

`.text.vectors` and `.text.stubs` are the exception vectors and stubs. The kernel maps these to the high vector addresses, 0xffff_0000 and 0xffff_1000 respectively.

`.data.id_pages` and `.data.pages` are blocks reserved for the [initial kernel page tables](#arm-initial-page-tables). The kernel requires Large Physical Address Extensions and reserves three pages for each LPAE table.

#### Operating Mode

The bootloader will have already put the primary core into SVC or HYP. On startup, Propeller ensures the primary core is in SVC before performing startup tasks. If the primary core is in an unexpected mode initially, Propeller halts.

#### Basic Startup

Once in SVC on the primary core, Propeller sets the primary core's `SP_svc` pointer to `__kernel_svc_stack_start` so that it can start calling helper functions using the [AArch32 procedure call standard][aarch32proccall].

With the stack set, Propeller writes all zeros to the `.bss` section.

Next, Propeller checks if the blob provided by the bootloader is a DeviceTree by checking if the first four bytes are the DeviceTree magic bytes. Propeller *only* supports DeviceTrees. If the blob is not a DeviceTree, Propeller halts.

#### Initial Page Tables {#arm-initial-page-tables}

`__virtual_start` is a compile-time constant provided by the linker script specifying the virtual address base of the kernel. The virtual base depends on the 32-bit address space split. Propeller supports the canonical 2/2 and 3/1 splits.

Because Propeller has no idea how much memory actually exists in the system at this point, it takes a very conservative approach to the initial page tables. The kernel image and the DeviceTree binary (DTB), if present, are linearly mapped in 2 MiB sections. The identity tables map the physical addresses back to the same physical address while the virtual address page tables map the physical addresses offset by `__virtual_start`.

Each table has three pages, one for each of the L1, L2, and L3 LPAE tables. Only the first entries of the L1 table is used for the first 1 GiB of the virtual address space. The 2 MiB sections of the kernel image and DTB are mapped in the L2 table.

          Identity              Virtual
          Map                   Map

    PE +---------------+     +---------------+ VS + PE
       | DTB           |     | DTB           |
    PS +---------------+     +---------------+ VS + PS
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
| `VS`         | `__virtual_start`                        |
| `KS`         | `__kernel_start`                         |
| `KE`         | ⌈ `__kernel_size` ⌉~2MiB~                |
| `PS`         | Blob pointer provided by the bootloader. |
| `PE`         | ⌈ Blob Size ⌉~2MiB~                      |

The identity tables allow a core to find the next instruction, typically a jump to set the program counter to virtual addressing, after enabling the MMU. After making the jump to virtual addressing, Propeller sets `TTBR0` back to 0.

The identity tables are placed in the kernel image prior to the virtual tables to ensure they remain intact for the secondary cores.

#### Transfer to Kernel Initialization

After enabling the MMU, the primary core fills out the ARM kernel configuration struct and passes it to `pk_init` entry point. All addresses in the struct are physical.

    +---------------------------------+ 44
    | Physical primary stack address  |
    +---------------------------------+ 40
    | ISR stack page count            |
    +---------------------------------+ 36
    | ISR stack list address          |
    +---------------------------------+ 32
    | Virtual memory split            |
    +---------------------------------+ 28
    | Page table area size            |
    +---------------------------------+ 24
    | Physical page tables address    |
    +---------------------------------+ 20
    | Kernel size                     |
    +---------------------------------+ 16
    | Physical kernel address         |
    +---------------------------------+ 12
    | Physical blob address           |
    +---------------------------------+ 8
    | Page size                       |
    +---------------------------------+ 4
    | Virtual base address            |
    +---------------------------------+ 0

#### CPU Initialization

Propeller scans the DTB for a list of logical cores and their thread IDs. Propeller builds a core database indexed by order in which the cores appear in the DTB. The primary core is the exception; it is always assigned index 0 regardless of its thread ID.

`MPIDR` allows for non-contiguous, hierarchical thread IDs, so this internal index is used as a contiguous, zero-based number used for the kernel's data structures (e.g. the ISR stack table). Propeller uses the affinity value specified the DTB `reg` tag for each core, so it is imperative that this value match the affinity values provided by `MPIDR` on each core. The core database provides reverse lookup from `MPIDR` affinity value to core index.

ARM builds of Propeller are limited to [16 cores](#thread-local-area), and will only add the first 16 cores it encounters in the DTB to the core database.

After initializing the core database, Propeller initializes a statically-allocated task structure called the Bootstrap Task and provides the Bootstrap Task with a statically-allocated page table for local mappings. This Bootstrap Task represents the single-thread boot code and allows mapping the High Memory area to set up the allocator data structures before going multithreaded. Once single-threaded initialization has completed, the Bootstrap Task will be replaced by the real Init Task.

#### Memory Initialization

#### Address Space

Propeller can use a canonical 32-bit 3/1 or 2/2 split configuration:

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

Not all ARM CPUs support the Large Physical Address Extensions required for the 3/1 split, however Propeller requires LPAE and will not boot otherwise.

The split is a balance of available physical memory versus speed. The ARM supports AArch64 and should run a 64-bit build of Propeller if it has more than 2 GiB of memory. With less than 2 GiB, a 2/2 is the most performant option. When using a 3/1 split with more than 1 GiB of memory, Propeller will use the [Linux High Memory Handling][linuxhighmem] method of per-thread temporary memory mapping to access memory beyond 896 MiB in the kernel.

When using a 3/1 split configuration, Propeller creates a Linear Memory area with a fixed, linear mapping to the first 896 MiB of physical memory starting at the kernel segment's base address.

    +-----------------+ 0xffff_ffff    -+
    | / / / / / / / / | 56 KiB          |
    |.................| 0xffff_2000     |
    | Exception Stubs | 4 KiB           |
    |.................| 0xffff_1000     |
    | Vectors         | 4 KiB           |
    |.................| 0xffff_0000     |    K
    | / / / / / / / / | 1,984 KiB       |    E
    |.................| 0xffe0_0000     |    R
    | Recursive Map   | 2 MiB           |    N
    |.................| 0xffc0_0000     |    E
    | Page Database   | 24 MiB          |    L
    |.................| 0xfe40_0000     |
    | ISR Stacks      |                 |    S
    |.................|                 |    E
    | Thread Local    |                 |    G
    |.................|                 |    M
    |                 |                 |    E
    | Hardware Area   |                 |    N
    |                 |                 |    T
    +-----------------+ 0xf800_0000     |
    |                 |                 |
    |                 |                 |
    | Linear Mappings | 896 MiB         |
    |                 |                 |
    |                 |                 |
    +-----------------+ 0xc000_0000    -+
    |                 |
    |                 |
    | User Segment    | 3 GiB
    |                 |
    |                 |
    +-----------------+ 0x0000_0000

When using a 2/2 split configuration, Propeller maps the first 1,920 MiB of physical memory starting at the kernel's base address and uses the top 128 MiB in the same manner as a 3/1 split.

##### Exception Vectors and Stubs

Propeller configures ARM cores to place exception vectors at 0xffff_0000 and places the stub pointers in the following page at 0xffff_10000. The top 56 KiB of the address space are unused.

##### Recursive Map Area

The Recursive Map area provides access to the page tables that map the upper 1 GiB of the kernel's address space. With a 3/1 split, this will be all the kernel's page tables. With a 2/2 split, the page tables that map the lower 1 GiB of the kernel's address space will not be accessible through the Recursive Map area.

Refer to [Recursive Page Tables][recursivemap].

An example Level 2 table that covers the upper 1 GiB of the kernel's address space is set up as follows:

    +----------------------------------+
    | Level 2 Table 0xaaaa_0000        |
    +-----+----------------------------+ <----+
    | 0   | Level 3 Table 0xbbbb_0000  |      |
    +-----+----------------------------+      |
    | 1   |                            |      |
    | ... | Other Mappings             |      |
    | 509 |                            |      |
    +-----+----------------------------+      |
    | 510 | Recursive to 0xaaaa_0000   | -----+
    +-----+----------------------------+
    | 511 | Vector Mappings            |
    +-----+----------------------------+

Entry 510 is a recursive mapping back to the beginning of the Level 2 table and reserves the 2 MiB block at 0xffc0_0000 for page table access. Consider the virtual address 0xffc0_0000:

      11   111111110   000000000   000000000000
    +----+-----------+-----------+--------------+
    | L1 |    L2     |    L2     |      L3      |
    +----+-----------+-----------+--------------+
    31  30          21          12              0

Bits [31:30] select the Level 2 table that covers the upper 1 GiB as normal. Bits [29:21] have a value of 0x1fe to select entry 510. This means the core jumps back to the *same* Level 2 table, but will *think* it is at a Level 3 table. Bits [20:12] select entry 0, and the core jumps to the Level 3 table at 0xbbbb_0000. The magic is that translations stops there. So, bits [11:0] are now offsets into the Level 3 table.

Consider the virtual address 0xffdf_e000:

      11   111111110   111111110   000000000000
    +----+-----------+-----------+--------------+
    | L1 |    L2     |    L2     |      L2      |
    +----+-----------+-----------+--------------+
    31  30          21          12              0

After the first recursion, bits [20:12] again select entry 510 in the Level 2 table, the core jumps back to the *same* Level 2 table, and translation stops. Bits [11:0] are now offsets into the same Level 2 table.

##### Page Database

The 32 MiB Page Database area is a virtually-contiguous array of page metadata entries. With 4 KiB pages, the 3 GiB of addressable memory has 768 Ki pages. 24 MiB allows for 32 bytes of metadata for each page.

Why 32 bytes? Will we need more? Great questions! Anyway...

Similar to the Linux FLATMEM virtual memory map model, this simplifies conversion from a page metadata address to a page physical address and vice versa. For 4 KiB pages:

    Page Frame Number (PFN) = Physical Address >> 12
    Page Metadata Address   = ( PFN << 5 ) + 0xfdc0_0000

The process is easily reversed to calculate a page physical address from a page metadata address.

We are not worrying about NUMA architectures, just UMA. So, a model similar to the Linux SPARSEMEM model is not really necessary. This model is also similar to the Windows NT Page Frame Database circa 1990 per the Windows Research Kernel documents.

##### ISR Stacks

The ISR Stacks area virtually maps each core's ISR stacks with unmapped guard pages in between each to trap stack overflows. With the maximum of 16 cores, 4 stacks per core (SVC, ABT, IRQ, FIQ), a page size of 4 KiB, and the default 2-page stack, the maximum ISR Stacks area size is 768 KiB with guard pages. The actual size is determined at boot when the number of cores, stack size, and page size are known.

##### Thread Local Area

The Thread Local area is reserved for mapping per-thread page tables that map upper memory beyond the linear mappings. Each kernel thread has its own Level 3-page table that is mapped when activating the thread and allows the thread to temporarily map 2 MiB of pages into the Thread Local area.

Each core is assigned a 2 MiB block within the Thread Local area, Propeller limits ARM builds to 16 cores to ensure the Thread Local area is never larger than 32 MiB. When a thread has local mappings, the kernel will pin the task to that core until unmaps all of its local mappings. This ensures the thread's pointers remain valid across context switches.

The Thread Local area is aligned on a 2 MiB boundary

Threads store the physical address of their thread-local table in their context struct. When switching threads, the physical address is mapped to the core's assigned 2 MiB block. Once mapped, the table is accessible for updating through the Recursive Map area. For example: Assume there are 16 cores and the Thread Local area is 32 MiB. The Thread Local area base will be 0xfbc0_0000, and Core 1's 2 MiB block will start at 0xfbe0_0000, or entry 479 (0x1df). After putting the physical address of the thread-local page table into entry 479, the thread-local page table itself can be edited using the addresses [0xffdf_f000, 0xffe0_0000).

      11   111111110   111011111   xxxxxxxxxxxx
    +----+-----------+-----------+--------------+
    | L1 |    L2     |    L2     | Thread Local |
    +----+-----------+-----------+--------------+
    31  30          21          12              0

##### Hardware Area

There is current nothing interesting going on here. Likely, Propeller will allow drivers to permanently map their devices into this area.

#### Multi-Core Initialization

See AArch64 [Multi-Core Initialization](#aarch64-multi-core-init). The primary difference is that each ARM core will have SVC, ABT, IRQ, and FIQ entries in the kernel stack list. Otherwise, the multi-core initialization concepts are the same.

    ...                         ...
     +---------------------------+
     | Core X SVC Stack Address  |
     +---------------------------+ +16
     | Core X ABT Stack Address  |
     +---------------------------+ +12
     | Core X IRQ Stack Address  |
     +---------------------------+ +8
     | Core X FIQ Stack Address  |
     +---------------------------+ +4
     | Core X Thread ID          |
     +---------------------------+ virtual base + list address + 20 * X
    ...                         ...

### AArch64 Implementation {#aarch64-arch-impl}

#### Page Size

`__page_size` is a compile-time constant provided by the linker script that specifies the size of a page. Propeller will panic if the page size is not 4 KiB.

#### Kernel Image Layout

    +----------------------+ __kernel_start / __text_start
    | .text                |
    +----------------------+ __rodata_start
    | .rodata              |
    +----------------------+ __data_start
    | .data                |
    +----------------------+ __bss_start
    | .bss                 |
    +----------------------+ __kernel_stack_end
    | .data.stack          | 
    +----------------------+ __kernel_stack_list
    | .data.stack_pointers |
    +----------------------+ __kernel_id_pages_start
    | .data.id_pages       |
    +----------------------+ __kernel_pages_start
    | .data.pages          |
    +----------------------+ __kernel_end

The base of the `.text` segment is specified by the compile-time constant `__kernel_start` provided by the build system.

`.data.stack` is the primary core's interrupt service routine (ISR) stack. Refer to `SP_EL1`. `__kernel_stack_pages` is a compile-time constant provided by the linker script that specifies the ISR stack size in pages. During the single-threaded setup phase, the primary core uses this stack as its general purpose stack.

`.data.stack_pointers` is the ISR stack pointer table for secondary cores. During the single-threaded setup phase, the primary core will allocate pages for secondary core ISR stacks and place pointers to the tops of those stacks in this table. The secondary cores will index this table to locate their stacks when they are released.

The stack pointer table is a single page of 512 8-byte pointer entries. 512 entries is sufficient for the 256 core maximum on AArch64 nodes. See [Multi-Core Initialization](#aarch64-multi-core-init).

`.data.id_pages` and `.data.pages` are blocks reserved for the [initial kernel page tables](#aarch64-initial-page-tables). The kernel image reserves three pages for each table.

#### Exception Level

The bootloader will have already put the primary core into EL2 or EL1. On startup, Propeller ensures the primary core is in EL1 before performing startup tasks. If the primary core is in an unexpected mode initially, Propeller halts.

#### Basic Startup

Once in EL1 on the primary core, Propeller sets the primary core's stack pointer to `__kernel_stack_start` so that it can start calling helper functions using the [AArch64 procedure call standard][aarch64proccall].

With the stack set, Propeller writes all zeros to the `.bss` section.

Next, Propeller checks if the blob provided by the bootloader is a DeviceTree by checking if the first four bytes are the DeviceTree magic bytes. Propeller *only* supports DeviceTrees. If the blob is not a DeviceTree, Propeller halts.

#### Initial Page Tables

`__virtual_start` is a compile-time constant provided by the linker script specifying the virtual address base of the kernel.

Because Propeller has no idea how much memory actually exists in the system at this point, it takes a very conservative approach to the initial page tables. The kernel image and the DeviceTree binary (DTB), if present, are linearly mapped in 2 MiB sections. The identity tables map the physical addresses back to the same physical address while the virtual address page tables map the physical addresses offset by `__virtual_start`.

Each table has three pages, one for each of the L1, L2, and L3 tables. Only the first entries of the L1 and L2 tables are used for the first 1 GiB of the virtual address space. The 2 MiB sections of the kernel image and DTB are mapped in the L3 table.

          Identity              Virtual
          Map                   Map

    PE +---------------+     +---------------+ VS + PE
       | DTB           |     | DTB           |
    PS +---------------+     +---------------+ VS + PS
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
| `VS`         | `__virtual_start`                        |
| `KS`         | `__kernel_start`                         |
| `KE`         | ⌈ `__kernel_size` ⌉~2MiB~                |
| `PS`         | Blob pointer provided by the bootloader. |
| `PE`         | ⌈ Blob Size ⌉~2MiB~                      |

The identity tables allow a core to find the next instruction, typically a jump to set the program counter to virtual addressing, after enabling the MMU. After making the jump to virtual addressing, Propeller sets `TTBR0_EL1` back to 0.

The identity tables are placed in the kernel image prior to the virtual tables to ensure they remain intact for the secondary cores.

#### Transfer to Kernel Initialization

After enabling the MMU, the primary core fills out the AArch64 kernel configuration struct and passes it to the `pk_init` entry point. All addresses in the struct are physical.

    +---------------------------------+ 80
    | Physical primary stack address  |
    +---------------------------------+ 72
    | ISR stack page count            |
    +---------------------------------+ 64
    | ISR stack list address          |
    +---------------------------------+ 56
    | Page table area size            |
    +---------------------------------+ 48
    | Physical page tables address    |
    +---------------------------------+ 40
    | Kernel size                     |
    +---------------------------------+ 32
    | Physical kernel address         |
    +---------------------------------+ 24
    | Physical blob address           |
    +---------------------------------+ 16
    | Page size                       |
    +---------------------------------+ 8
    | Virtual base address            |
    +---------------------------------+ 0

#### CPU Initialization

Propeller scans the DTB for a list of logical cores and their thread IDs. Propeller builds a core database indexed by order in which the cores appear in the DTB. The primary core is the exception; it is always assigned index 0 regardless of its thread ID.

`MPIDR_EL1` allows for non-contiguous, hierarchical thread IDs, so this internal index is used as a contiguous, zero-based number used for the kernel's data structures (e.g. the ISR stack table). Propeller uses the affinity value specified the DTB `reg` tag for each core, so it is imperative that this value match the affinity values provided by `MPIDR_EL1` on each core. The core database provides reverse lookup from `MPIDR` affinity value to core index.

AArch64 builds of Propeller are limited to 256 cores, and will only add the first 256 cores it encounters in the DTB to the core database. Unlike the 16-core limitation on ARM builds, this is an arbitrary limitation. However, increasing it does increase the memory cost of the kernel's data structures.

After initializing the core database, Propeller initializes a statically-allocated task structure called the Bootstrap Task. Unlike ARM builds, the AArch64 Bootstrap Task is simply a placeholder to satisfy thread-local page mapping interface. The Bootstrap Task implementation is structured such that mapping can be optimized by the compiler to an addition and unmapping can be optimized away.

#### Memory Initialization

#### Address Space

Propeller uses the canonical 256 TiB arrangement for a 64-bit address space and allows up to just under 254 TiB of physical memory accessed through a fixed, linear mapping.

    +-----------------+ 0xffff_ffff_ffff_ffff    -+
    | Page Database   | 2 TiB                     |    K S
    |.................| 0xffff_fe00_0000_0000     |    E E
    | ISR Stacks      |                           |    R G
    |.................|                           |    N M
    |                 |                           |    E E
    | Fixed Mappings  |                           |    L N
    |                 |                           |      T
    +-----------------+ 0xffff_0000_0000_0000    -+
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

##### Exception Vectors

The exception vectors are part of the kernel image. Unlike ARM, AArch64 can set the vector table to an arbitrary address.

##### Page Database

The 2 TiB Page Database area is a virtually-contiguous array of page metadata entries. With 4 KiB pages, the 256 TiB address space has 64 Gi pages. 2 TiB allows for 32 bytes of metadata for each page.

Why 32 bytes? Will we need more? Great questions! Anyway...

Similar to the Linux FLATMEM virtual memory map model, this simplifies conversion from a page metadata address to a page physical address and vice versa. For 4 KiB pages:

    Page Frame Number (PFN) = Physical Address >> 12
    Page Metadata Address   = ( PFN << 5 ) + 0xffff_fffe_0000_0000

The process is easily reversed to calculate a page physical address from a page metadata address.

We are not worrying about NUMA architectures, just UMA. So, a model similar to the Linux SPARSEMEM model is not really necessary. This model is also similar to the Windows NT Page Frame Database circa 1990 per the Windows Research Kernel documents.

Reserving 2 TiB at the top of the kernel segment means the actual maximum on addressable memory is roughly 254 TiB. "Roughly" because the ISR Stack Area grows with the number of cores, but to a very small degree compared to 2 TiB.

##### ISR Stacks

The ISR Stacks area virtually maps each core's ISR stack with unmapped guard pages in between each to trap stack overflows. With the maximum of 256 cores, a page size of 4 KiB, and the default 2-page stack, the maximum ISR Stacks area size is 3 MiB with guard pages. As of 2023, Ampere Computing is starting to push single-node core counts to 384 cores ([2 sockets each with 192 cores][largecorcount]). Even if someone pushes to 1,024, that would still only be 12 MiB of stack space.

#### Multi-Core Initialization {#aarch64-multi-core-init}

Before releasing secondary cores, Propeller allocates the ISR stacks, maps them into the ISR Stack area, and fills out the kernel stack list. Each entry in the kernel stack list is a pair of words: the thread ID of the core at that index and the core's stack pointer. The secondary cores will search the list for their hardware ID since obtaining their index is not trivial and requires a stack.

     +---------------------------+
     | Core N ISR Stack Address  |
     +---------------------------+ +16 * N + 8
     | Core N Thread ID          |
     +---------------------------+ +16 * N
    ...                         ...
     +---------------------------+ +32
     | Core 1 ISR Stack Address  |
     +---------------------------+ +24
     | Core 1 Thread ID          |
     +---------------------------+ +16
     | Core 0 ISR Stack Address  |
     +---------------------------+ +8
     | Core 0 Thread ID          |
     +---------------------------+  virtual base + list address

While the primary core's ISR stack is physically located in the kernel image, Propeller remaps it into the ISR Stacks area with a guard page and updates the stack pointer. The stacks for the remaining cores are dynamically allocated and mapped into the ISR Stacks area once Propeller initializes the page allocators. The "step size" in the example below is the stack size plus one page.

Note that the primary core's stack starts at the end of the ISR stack area. This simplifies early setup of the primary core's stack when the number of cores is not known.

     +---------------------------+ +step size
     | Core 0 ISR Stack          |
     +---------------------------+
     | / / / / / Guard / / / / / |
     +---------------------------+ +step size * N
     | Core N ISR Stack          |
     +---------------------------+
     | / / / / / Guard / / / / / |
     +---------------------------+
    ...                         ...
     +---------------------------+ +step size * 2
     | Core 2 ISR Stack          |
     +---------------------------+
     | / / / / / Guard / / / / / |
     +---------------------------+ +step size
     | Core 1 ISR Stack          |
     +---------------------------+
     | / / / / / Guard / / / / / |
     +---------------------------+  ISR Stack area base

After the ISR stacks have been allocated and mapped, Propeller will unpark the secondary cores by vectoring them to `_secondary_start`. 

## `mm` Module

### Pager

### Buddy Allocator

Refer to [Buddy Allocator][buddyalloc].

A buddy allocator manages an area of physical memory and allocates blocks of up to 2^10 pages. Buddy allocators coarse allocators that should be used to allocate large blocks for kernel object (slab) allocators and granular, core-local allocators. Buddy allocators are not thread-safe and they do not protect against double-frees.

The buddy allocator has a small amount of overhead to track buddy pair state. The state is a contiguous array of bits representing buddy pairs of pages. When a bit is 0, either both pages are free or both are allocated. When a bit is 1, only one page in the buddy pair is allocated.

On a system with 1 GiB of physical memory and 4 KiB pages, the buddy allocator needs just shy of 32 KiB for the buddy pair state. Out of the 256 Ki pages available, the buddy allocator will reserve 8 of them for the metadata. This equates to about 0.003% overhead. This overhead holds with scale. A 3 GiB range requires 24 pages (~96 KiB) out of 768 Ki pages, and a 256 TiB range requires 2,047 Ki pages (~8 GiB) out of 64 Gi pages.

The index of a page's bit is simply `(PFN - PFN~0~) >> 1` where `PFN~0~` is the page frame number of the first page in the allocator's area.

An allocator may manage multiple discontinuous areas of memory so long as the state area is a contiguous array that includes the unused areas. For example, if an allocator manages the ranges [a:b] and [c:d] where `c > b`, then state memory must be reserved assuming the allocator is managing the range [a:d]. The allocator will never touch the bits in (b:c), so another allocator managing that area is free to use them. Otherwise, they will simply be unused overhead.

Propeller stores the state data for all allocators in linear memory so that the allocators do not need to perform any mapping operations to access the state.

During initialization, the buddy allocator embeds a linked list of free blocks for each order directly into the blocks themselves. Each field in the linked list structure is pointer-sized.

    +-------------------+ Block Size
    | / / / / / / / / / |
    | / / / / / / / / / |
    | / / / / / / / / / |
    +-------------------+
    | Checksum          |
    +-------------------+
    | Previous Pointer  |
    +-------------------+
    | Next Pointer      |
    +-------------------+ 0

When a block is free, the allocator calculates `Random Seed ⊕ Next Pointer ⊕ Previous Pointer` and stores the result in the block's Checksum field as a sanity check.

## `support` Module

## `sync` Module

[armbootproto]: https://www.kernel.org/doc/Documentation/arm/booting.rst
[aarch64bootproto]: https://www.kernel.org/doc/Documentation/arm64/booting.txt
[aarch32proccall]: https://github.com/ARM-software/abi-aa/blob/main/aapcs32/aapcs32.rst
[aarch64proccall]: https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst
[linuxmemmodels]: https://lwn.net/Articles/789304/
[buddyalloc]: https://en.wikipedia.org/wiki/Buddy_memory_allocation
[linuxhighmem]: https://docs.kernel.org/mm/highmem.html
[recursivemap]: https://os.phil-opp.com/paging-implementation/#recursive-page-tables
[largecorecount]: https://www.tomshardware.com/pc-components/cpus/yes-you-can-have-too-many-cores-amperes-192-core-cpus-break-arm64-linux-kernel-in-two-socket-systems-company-requests-higher-core-count-support-for-mainline-linux