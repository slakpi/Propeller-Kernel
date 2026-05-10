# THE PROPELLER KERNEL - PART 7

## Introduction

Hopefully, you were able to get the MMU enabled and maybe even explored the 32-bit ARM MMU set up code as well. If you were able to get the MMU enabled, you may have noticed that there are a few more things we need to do after jumping to virtual addressing.

## Exception Primer

It might be worth reading the [Handling Exceptions](https://developer.arm.com/documentation/102412/0103/Handling-exceptions/Taking-an-exception) section of the ARMv8 Programmer's Guide just to get a refresher on the program flow during an exception since we are going to be talking about laying down some of the foundational infrastructure for exception handling.

## Primary Core Stack

In [Part 5](https://slakpi.github.io/Propeller-Kernel/part_3.html#primary-core-boot), we set up a stack for the primary core by reserving space in the kernel image and setting SP to the physical address of the start of the stack. This gave us the ability to call some helper functions while setting up the MMU. Now that the MMU is on, however, we need to update the stack pointer to a virtual address. We also discussed the possibility of a stack overflow since there is no safeguard for writing beyond the bottom of the stack. Before we address any of that, we need to talk a little more about how AArch64 and ARM handle stacks.

## ARM

Let’s talk about the more complicated of the two first. ARMv7 has several modes, and each of those modes has a banked stack pointer register. See: [ARM Processor Modes and Registers](https://developer.arm.com/documentation/den0013/0400/ARM-Processor-Modes-and-Registers/Registers).

When we first boot on ARM, we ensure the core is in SVC mode. So, when we set the `SP` register, we actually set the `SP_svc` register. There are also stack pointers for FIQ (high-priority "fast" interrupt), UND (undefined instruction exception), ABT (abort exception), IRQ (interrupt), and HYP (hypervisor mode). SYS (system or kernel mode) and USR (user mode) actually share the same stack pointer.

Each mode has its own stack pointer to ensure the core can continue even if the user or system stacks are bad. For example, the kernel has a stack overflow causing an ABT exception. When the abort occurs and the core enters ABT mode, it has a valid stack the handler can use to deal with the issue.

> Interestingly, Figure 2 in [Section 3.1](https://developer.arm.com/documentation/den0013/0400/ARM-Processor-Modes-and-Registers/Registers) of the ARMv7 Programmer's Guide shows IRQ using `SP_svc` instead of `SP_irq` while the narrative below it states: "For all modes other than User and System modes, R13 and the SPSRs are banked." Figure B1-2 in Section B1.3.2 of the ARMv7ar Reference Manual ***correctly*** shows IRQ using `SP_irq`.

When entering the kernel under normal conditions, a hardware or software interrupt for example, we will be in SVC, IRQ, or FIQ mode. Each has its own stack pointer, and can handle a hardware interrupt directly.

However, if we enter SVC mode via software interrupt for a system call, we need to be able to get the parameters for the system call off of the user task's stack. In this case, once the interrupt handler determines that it is handling a system call, it can put the core into SYS mode so that the USR stack is now in use but the core is still in a privileged mode.

## AArch64

As mentioned in [Part 5](https://slakpi.github.io/Propeller-Kernel/part_5.html#modes-privilege-levels-and-exception-levels), AArch64 simplifies exceptions. It also simplifies how stacks are managed during exceptions.

Recall that EL0 is user mode and EL1 is supervisor mode. For our purposes, we are not going to worry about EL2 hypervisor mode and EL3 secure mode.

Each exception level has its own stack pointer: `SP_el0`, `SP_el1`, `SP_el2`, and `SP_el3`. When we first boot on AArch64, we ensure the core is in EL1. So, when we set the `SP` register, we actually set the `SP_el1` register. When taking an exception to EL1, the core will automatically switch to using `SP_el1` for the same reasons stated above: the core needs to be sure it has a valid stack.

AArch64 automatically switches to `SP_elx` by setting bit 0 in [`SPSel`](https://arm.jonpalmisc.com/latest_sysreg/AArch64-spsel) to 1. For example, when taking an exception from EL0 to EL1, `SPSel[0]` is set to 1 and the interrupt handler has `SP_el1` available to it. If the exception was a software interrupt, the handler can set `SPSel[0]` back to 0 to access `SP_el0`.

## Moving the Stack to Virtual Addressing

Given the information above, mapping our current stack into virtual memory is going to be a little more complicated than just adding the virtual base address to the stack pointer.

We are going to need to reserve part of the kernel's virtual address space for the stacks. How much space do we need to reserve?

Recall that we currently have the problem of a stack overflow. Virtual addressing gives us the ability to take an exception if an attempt is made to write to an unmapped address. This gives us a good solution to the stack overflow problem: simply do not map the page before the stack. In the example below, we have a two page stack preceded by a page that does not exist in the translation tables.

     +---------------------------+ Stack Start
     | Page 2                    |
     |...........................|
     | Page 1                    |
     +---------------------------+ Stack Base
     | / / / / / Guard / / / / / |
     +---------------------------+

We specified in [Part 5](https://slakpi.github.io/Propeller-Kernel/part_5.html#expanding-the-kernel-image) that our ISR stacks are 2 pages. With the unmapped guard page, each stack will need 3 pages of virtual address space. For ARM, each core will have five stacks (FIQ, UND, ABT, IRQ, and SVC) totaling 15 pages. For AArch64, each core will need 3 pages for their EL1 stacks.

That just leaves the number of cores as the only variable. Currently, Linux supports a maximum of [512 cores](https://www.tomshardware.com/pc-components/cpus/yes-you-can-have-too-many-cores-amperes-192-core-cpus-break-arm64-linux-kernel-in-two-socket-systems-company-requests-higher-core-count-support-for-mainline-linux) for AArch64. So, setting a hard limit on the number of cores is a reasonable thing to do. In our case, limiting ARM to 16 cores and AArch64 to 256 will do.

Therefore, we will need to reserve 240 pages (960 KiB) for ARM and 768 pages (3 MiB) for AArch64. Keep in mind that we may not actually allocate that many stacks, but we do need to make sure we have an area in the kernel segment large enough if we do.

> You are probably wondering why ARM is limited to 16 cores, but AArch64 gets a lavish 256-core limit. This mostly has to do with the kernel's data structures being limited to the top 128 MiB of the kernel segment. However, there is an interesting discussion coming up about how the kernel accesses physical memory beyond the 896 MiB that are linearly mapped and how that affects the available kernel address space.

We are starting to build up a more detailed image of what the virtual address space is going to look like.

### ARM

	+-----------------+ 0xffff_ffff    -+
    |                 |                 |
    |.................| 0xfe40_0000     |
    | ISR Stacks      |                 |    K S
    |.................|                 |    E E
    |                 |                 |    R G
    +-----------------+ 0xf800_0000     |    N M
    |                 |                 |    E E
    |                 |                 |    L N
    | Linear Mappings | 896 MiB         |      T
    |                 |                 |
    |                 |                 |
    +-----------------+ 0xc000_0000    -+
    |                 |
    |                 |
    | User Segment    | 3 GiB
    |                 |
    |                 |
    +-----------------+ 0x0000_0000

For ARM, we will reserve the area from 0xfe31_0000 - 0xfe40_0000 for the stacks. Do not worry about why these specific addresses right now. We will fill in the blanks later.

### AArch64

    +-----------------+ 0xffff_ffff_ffff_ffff    -+
    |                 |                           |    K S
    |.................| 0xffff_fe00_0000_0000     |    E E
    | ISR Stacks      |                           |    R G
    |.................|                           |    N M
    |                 |                           |    E E
    | Linear Mappings |                           |    L N
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

For AArch64, we will reserve the area from 0x0xffff_fdff_ffd0_0000 to 0xffff_fe00_0000_0000 for the stacks. Again, we will fill in the blanks to explain the address choice later.

## Reserving Space in the Kernel Image for the ARM Stacks

Now that we know where to place the stacks, let's expand our ARM linker script for the additional stacks. In [Part 5](https://slakpi.github.io/Propeller-Kernel/part_5.html#expanding-the-kernel-image), we reserved space in the kernel image for a single stack. That is fine for AArch64, but we need five stacks for ARM. We can modify the ARM linker script slightly:

```
  .data.stacks : ALIGN(__page_size)
  {
    . += (5 * __kernel_stack_pages * __page_size);
    __kernel_svc_stack_start = .;
  }
```

## Mapping the Stacks

Mapping the stacks is just a matter of creating a new translation table and doing work very similar to what we did in [Part 6](https://slakpi.github.io/Propeller-Kernel/part_6.html#step-2c-building-the-tables) to build the initial translation tables.

[`mmu_setup_primary_core_stacks`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/mm.s#L305) looks pretty complicated, but that is just the nature of assembly. All it really does is map a free Level 3 table, finds the base of the FIQ stack, then performs a loop to add entries for the five ARM stacks taking care to skip the guard pages.

When it is done, the stacks will be mapped in this order:

    +---------------------------+ 0xfe40_0000
    | SVC Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+
    | IRQ Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+
    | ABT Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+
    | IRQ Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+
    | FIQ Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+ 0xfe3f_1000

[`mmu_setup_primary_core_stack`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/mm.s#L214) does the same thing for AArch64, but only needs to add entries for the single stack.

When it is done, the EL1 stack will be mapped as:

    +---------------------------+ 0xffff_fe00_0000_0000
    | SVC Stack                 |
    +---------------------------+
    | / / / / / Guard / / / / / |
    +---------------------------+ 0xffff_fdff_ffff_d000

Both functions do something a little sneaky, however. Recall that a function prologue saves the current stack pointer to the frame pointer register and a function epilogue restores the stack pointer. Both functions save the *virtual* SVC / EL1 stack pointer to frame pointer and restore that value at the end of the function.

Ok! Now we just need to actually do the mapping right after we enable the MMU.

### ARM

For ARM, add a define at the beginning of `start.s` with the starting address of the SVC stack:

```assembly
.equ PRIMARY_SVC_STACK_START, 0xfe400000
```

Then call `mmu_setup_primary_core_stacks` after enabling the MMU:

```assembly
primary_core_begin_virt_addressing:
// ISR stack setup with virtual addressing enabled. This has to be done while
// the identity tables are still valid and the stack is empty.
  ldr     r0, =PRIMARY_SVC_STACK_START
  ldr     r1, =__vmsplit
  bl      mmu_setup_primary_core_stacks
```

### AArch64

For AArch64, add a define at the beginning of `start.s` with the starting address of the EL1 stack:

```assembly
.equ PRIMARY_STACK_START, 0xfffffe0000000000
```

Then call `mmu_setup_primary_core_stack` after enabling the MMU:

```assembly
primary_core_begin_virt_addressing:
// ISR stack setup with virtual addressing enabled. This has to be done while
// the identity tables are still valid and the stack is empty.
  ldr     x0, =PRIMARY_STACK_START
  bl      mmu_setup_primary_core_stack
  
// Halt
1:
  wfi
  b       1b
```

## Testing

Hopefully, you have not had any issues with debugging through QEMU. So, go ahead and launch QEMU with the ARM kernel and connect GDB. Try this command:

```
(gdb) layout asm
```

This should give you split window with an empty assembly listing at the top. Now use the `focus next` command to move focus from the assembly listing to the command window. That way, you can use the up/down arrows to cycle through commands rather than scroll the assembly listing.

```
(gdb) focus next
```

Now set a breakpoint right after the MMU is enabled:

```
(gdb) b primary_core_begin_virt_addressing
```

Use the `c` command to run the kernel to the breakpoint. If all goes well, GDB will break right after the MMU is enabled and give you an assembly listing at the top.

The stack does not have a frame information GDB can use to understand the scope of a function, so set a breakpoint at the virtual address of the `wfi` instruction after the call to `mmu_setup_primary_core_stacks`. Use the `c` command to skip to the breakpoint.

At this point, the stacks for all of the ARM exception modes should be mapped into virtual memory and we can test it. First, if we print the stack pointer, we should see the top of `SP_svc`.

```
(gdb) p/x $sp
$1 = 0xfe400000
```

Next, we can probe the mapped pages. We should *not* be able to access the page at 0xfe40_0000 because that is beyond the top of the stack. We should be able to access the pages at 0xfe3f_f000 and 0xfe3f_e000, the two pages that make up the stack. And we should *not* be able to access 0xfe3f_d000 since that is guard page.

```
(gdb) x/1wx 0xfe400000
0xfe400000:     Cannot access memory at address 0xfe400000
(gdb) x/1wx 0xfe3ff000
0xfe3ff000:     <some value from the stack>
(gdb) x/1wx 0xfe3fe000
0xfe3fe000:     <some value from the stack>
(gdb) x/1wx 0xfe3fd000
0xfe3fd000:     Cannot access memory at address 0xfe3fd000
```

Let's probe the other side. FIQ is the lowest in memory starting at 0xfe3f4000.

```
(gdb) x/1wx 0xfe3f4000
0xfe3f4000:     Cannot access memory at address 0xfe3f4000
(gdb) x/1wx 0xfe3f3000
0xfe3f3000:     <some value from the stack>
(gdb) x/1wx 0xfe3f2000
0xfe3f2000:     <some value from the stack>
(gdb) x/1wx 0xfe3f1000
0xfe3f1000:     Cannot access memory at address 0xfe3f1000
```

You can try probing the other stacks as well. If you get similar results for all of the stacks, then you are really cooking!

Time for another break. In the next part, we will cleanup from setting up the MMU, assign the ARM stacks to their respective processor modes, and set up exception vectors!

-----
[Part 8](https://slakpi.github.io/Propeller-Kernel/part_8.html)

© Randy Widell