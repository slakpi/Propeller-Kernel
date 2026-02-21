# THE PROPELLER KERNEL - PART 5

## Introduction

Let's get down to some real work. This part of the tutorial is going to center around getting ready to enable the MMU and jump in to Rust code. Since there is a relatively large amount of assembly code involved, I am just going to provide pseudocode here and links to the actual Propeller code.

## Modes, Privilege Levels, and Exception Levels

One of the first things you need to wrap your mind around is privilege. [Privilege levels](https://en.wikipedia.org/wiki/Protection_ring#Modes) were a major advancement in processors that allow them to protect memory and functionality from faults or malicious code.

ARMv7 has a fairly complicated system of [processor modes or privilege levels](https://developer.arm.com/documentation/ddi0406/cb/System-Level-Architecture/The-System-Level-Programmers--Model/ARM-processor-modes-and-ARM-core-registers/ARM-processor-modes).

The three main processor modes we are concerned with right now are User (USR), Supervisor (SVC), and Hypervisor (HYP). User mode is the least privileged running at Privilege Level 0 (PL0), Supervisor mode is the most privileged for a normal system running at PL1. Hypervisors overseeing a virtual machine running in Supervisor mode run at PL2. ARM includes other processor modes, such as IRQ, that share privilege levels with other modes.

The Linux ARM boot protocol requires that the boot loader place ARM processors in SVC mode before starting the kernel, but *may* place the processor in HYP mode if the processor has virtualization extensions.

AArch64 simplifies things a bit by simply using [exception levels](https://developer.arm.com/documentation/ddi0487/maa/-Part-D-The-AArch64-System-Level-Architecture/-Chapter-D1-The-AArch64-System-Level-Programmers--Model/-D1-1-Exception-levels). Exception Level 0 (EL0) is equivalent to ARM USR/PL0, and EL1 is equivalent to ARM SVC/PL1. If the processor implements virtualization, EL2 is equivalent to ARM HYP/PL2. EL3 may exist if the processor supports secure mode.

The Linux AArch64 boot protocol requires that the boot loader place AArch64 processors in *at least* EL1, but *recommends* EL2.

The key takeaways for now are:

* Our kernel is going to run in SVC mode or EL1.
* ARM processors might be in HYP or SVC mode.
* Aarch64 processors might be in EL2 or EL1.
* We will need to implement some mechanism to jump to SVC or EL1 if the processor is in HYP or EL2.

## Procedure Call Standard

Next up, we want to talk about the [ARM](https://github.com/ARM-software/abi-aa/blob/main/aapcs32/aapcs32.rst) and [AArch64](https://github.com/ARM-software/abi-aa/blob/main/aapcs64/aapcs64.rst) procedure call standards.

We need to adhere to a standard convention that defines how to pass parameters to a function and the responsibilities for preserving register values. Technically speaking, our assembly code can do whatever it wants, but some of our assembly code will be called by Rust code and the Rust compiler is going to expect our code to adhere to the standard.

### ARM Registers

ARM registers `r0` - `r3` are used for the first four integer arguments to a function. For example, if our assembly code wants to call the Rust function `fn foo( x: u32, y: u32 )`, it would place the value of `x` in `r0` and the value of `y` in `r1` before calling `foo`.

The argument registers are *caller*-saved, meaning the code calling a function is responsible for preserving any values in `r0` - `r4` that it wants to keep. The function being called is allowed to use those registers for its own purposes.

`r0` is typically the return register. For example, the Rust function `fn bar() -> u32` will place the return value in `r0`. However, all of the four argument registers may be used for return values. For example, the Rust function `fn baz() -> (u32, u32)` may place the first value of the tuple in `r0` and the second value in `r1`.

> *NOTE*: This is just a simple example of tuples to illustrate a point, and gets more complicated with heterogeneous data types.

`r4` - `r8` and `r10` are meant to store a function's variables. These registers are *callee*-saved, meaning the function being called is responsible for preserving any values in those registers. The function being called is free to use those registers for its own purposes, but must restore the original value before returning.

`r9` might have platform-specific uses. In our case, we are free to use it as a *callee*-saved register.

`r11` / `fp` is the Frame Pointer, `r13` / `sp` is the Stack Pointer, and `r14` / `lr` is the Link Register.

### AArch64 Registers

AArch64 uses `x` when using all 64-bits of a register and `w` when using only the bottom 32-bits of a register. Below, `r` will just be used as a stand-in for either `x` or `w`.

Registers `r0` - `r7` are *caller*-saved argument/result registers.

Registers `r9` - `r15` are *caller*-saved temporary registers.

Registers `r19` - `r28` are *callee*-saved registers.

`r29` / `fp` is the Frame Pointer, `r30` / `lr` is the Link Register, and `r31` / `sp` is the Stack Pointer.

## Link Register

Both ARM and AArch64 use the Link Register to store the return address. Both platforms use the `bl` (Branch and Link) instruction to simultaneously store the return address in `lr` then branch to the function.

On ARM, you return from a function by just setting the `pc` to the value of the `lr`, for example:

```assembly
foo:
  ...
  mov     pc, lr            // Return to caller

bar:
  ...
  bl      foo               // Call foo
  ...
```

On AArch64, the `ret` instruction takes the place of a manual move of `lr` to `pc`, for example:

```assembly
foo:
  ...
  ret                       // Return to caller

bar:
  ...
  bl      foo               // Call foo
  ...
```

If you think about this a little bit more, you might ask: How does this scale if `foo` itself calls another function? Well, let's talk about...

## Stacks and Frames

ARM and AArch64 support the use of a descending stack to pass extra parameters to functions, preserve registers, and store local variables in a function. The `sp` starts at the top of the stack (the highest possible address), then decreases with every push and increases with every pop:

                             Push             Push             Pop
    +-----+ <- sp    +-----+          +-----+          +-----+
    |     |          |  0  |          |  0  |          |  0  |
    +-----+          +-----+ <- sp    +-----+          +-----+ <- sp
    |     |          |     |          |  1  |          |     |
    +-----+          +-----+          +-----+ <- sp    +-----+
    |     |          |     |          |     |          |     |
    +-----+          +-----+          +-----+          +-----+
    |     |          |     |          |     |          |     |
    +-----+          +-----+          +-----+          +-----+

A frame is the portion of the stack that a function is using. We use `fp` to remember the beginning of the current function's frame. One of the first things we want to do is push `fp` to the stack to preserve the caller's frame pointer, then copy the current `sp` over to remember the start of the callee's frame. This is known as a function "prologue".

                             Push fp
                             and set
                             fp to sp         Push
    +-----+ <- fp    +-----+          +-----+          +-----+
    | / / |          | / / |          | / / |          | / / |
    | / / |          | / / |          | / / |          | / / |
    +-----+ <- sp    +-----+          +-----+          +-----+
    |     |          | fp  |          | fp  |          | fp  |
    +-----+          +-----+ <- fp sp +-----+ <- fp    +-----+ <- fp
    |     |          |     |          |  1  |          |  1  |
    +-----+          +-----+          +-----+ <- sp    +-----+
    |     |          |     |          |     |          |  2  |
    +-----+          +-----+          +-----+          +-----+ <- sp

At the end of a function, we want to set `sp` back to `fp` to clean up the current function's local variables, then pop the caller's frame pointer off of the stack before returning. This is known as a function "epilogue".

                             Set sp
                             to fp            Pop fp
    +-----+          +-----+          +-----+ <- fp
    | / / |          | / / |          | / / |
    | / / |          | / / |          | / / |
    +-----+          +-----+          +-----+ <- sp
    | fp  |          | fp  |          |     |
    +-----+ <- fp    +-----+ <- fp sp +-----+
    |  1  |          |     |          |     |
    +-----+          +-----+          +-----+
    |  2  |          |     |          |     |
    +-----+ <- sp    +-----+          +-----+

If we also save `lr` in the prologue, and restore it in the epilogue, we solve the problem posed in the last section: How do you call a function from a function?

                             Set sp           Pop lr
                             to fp            and fp
    +-----+          +-----+          +-----+
    | lr  |          | lr  |          | lr  |
    +-----+          +-----+          +-----+
    | fp  |          | fp  |          | fp  |
    +-----+          +-----+          +-----+ <- fp
    | / / |          | / / |          | / / |
    | / / |          | / / |          | / / |
    +-----+          +-----+          +-----+ <- sp
    | lr  |          | lr  |          |     |
    +-----+          +-----+          +-----+
    | fp  |          | fp  |          |     |
    +-----+ <- fp    +-----+ <- fp sp +-----+
    |  1  |          |     |          |     |
    +-----+          +-----+          +-----+
    |  2  |          |     |          |     |
    +-----+ <- sp    +-----+          +-----+
                                              Jump to lr
                                              address

If we are writing a function that is guaranteed not to call another function or modify the stack, we can skip the prologue and epilogue to save some time.

Propeller's ARM and AArch64 start modules have the macros for the prologue and epilogue defined in [arm/start/include/abi.h](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/include/abi.h) and [aarch64/start/include/abi.h](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/include/abi.h).

## Intermission Recap

We now have some idea of what our kernel is going to have to do immediately upon booting:

1. Make sure the primary core is in SVC mode on ARM and EL1 on AArch64.
2. Setup a stack so that we can properly use the procedure call standard to call helper functions.
3. Zero the `.bss` section.
4. Set up and then enable the MMU.

There is still one issue brought up in Part 3 that we have not answered: We still have no idea how much physical memory is in the system and where it is, so where do we get the memory for a stack?

## Expanding the Kernel Image

We can actually reserve memory for the initial stack in the kernel image:

    +-----------------+
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

By adding another data section to the linker script, we can reserve some memory for a stack.

```
__page_size = 4096;
__kernel_stack_pages = 2;

...

SECTIONS
{
  ...

  .data.stack : ALIGN(__page_size)
  {
    . += (__kernel_stack_pages * __page_size);
    __kernel_stack_start = .;
  }
}
```

This new section reserves 8 KiB in the kernel image for a stack and defines the `__kernel_stack_start` constant that our assembly code can use.

Stacks for kernel threads *should be* relatively small compared to user thread stacks which, for example, default to 1 MiB on Windows. Compared to a user thread, a kernel thread should be doing as little as possible.

There is another problem here that we will resolve once we enable the MMU: the threat of a stack overflow. By making a stack part of the kernel image, the stack can overflow into the rest of the kernel.

We are going to be extremely conservative with the stack to avoid an early overflow because it will be incredibly hard to debug. Later, we will still need to be conservative with the stack, but an overflow will result in an immediate exception and be much easier to debug.

Stay tuned.

## ARM Processor Mode

The first thing we want to do when we enter the kernel at [start.s:44](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/start.s#L44) is save the ATAG/DTB blob pointer to `r10` where it will not be modified.

At [start.s:56](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/start.s#L56), we check the [CPSR](https://arm.jonpalmisc.com/latest_sysreg/AArch32-cpsr) register. If the processor is in HYP mode, we jump to `hyp_entry`. If the processor is in SVC mode, we jump directly to `primary_core_boot`. Otherwise, we jump to [`cpu_halt`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/cpu.s#L13) since the processor is in an unexpected mode.

In `hyp_entry` at [start.s:108](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/start.s#L108), we set [ELR_hyp](https://arm.jonpalmisc.com/latest_sysreg/AArch32-elr_hyp) to the address of `primary_core_boot`, then set the flags in [SPSR_hyp](https://arm.jonpalmisc.com/latest_sysreg/AArch32-spsr_hyp) to move the processor to SVC mode when performing an exception return.

Finally, we perform an `eret` to perform an exception return and jump to `primary_core_boot` in SVC mode.

## AArch64 Exception Level

Similar to ARM, the first thing we want to do when we enter the kernel at [start.s:61](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L61) is save the ATAG/DTB blob pointer to `w19` where it will not be modified.

At [start.s:73](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L73), we check the [CurrentEL](https://arm.jonpalmisc.com/latest_sysreg/AArch64-currentel) register. Depending on the current exception level, we jump to `el3_entry`, `el2_entry`, or `el1_entry`. If the exception level is unexpected, we jump to [`cpu_halt`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/cpu.s#L13).

[`el3_entry`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L120) configures levels lower than EL3 to be non-secure and configures EL2 to be AArch64. Additionally, configures [SPSR_el3](https://arm.jonpalmisc.com/latest_sysreg/AArch64-spsr_el3) to disable interrupts when entering EL3 and assign `EL2_sp` to EL2, then sets the exception return to `el2_entry` before performing an exception return.

[`el2_entry`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L136) configures EL1 to be AArch64. Additionally, configures [SPSR_el2](https://arm.jonpalmisc.com/latest_sysreg/AArch64-spsr_el2) to disable interrupts when entering EL2 and assign `EL1_sp` to EL1, then sets the exception return to `el1_entry` before performing an exception return.

[`el1_entry`](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/aarch64/start/start.s#L152) configures [SCTLR_el1](https://arm.jonpalmisc.com/latest_sysreg/AArch64-sctlr_el1) with the required reserved bits and leaves the MMU disabled before jumping to `primary_core_boot`.

## Primary Core Boot

Phew! You know what, that was a lot. Take a breath...

OK, back to work.

Let’s set up the stack pointer with the physical address of the stack area that we reserved in the linker script. For AArch64, this is going to be easy. For ARM, however, is a little weird at first.

We discussed PC-relative addresses in Part 3 and how the assembly code would use offsets for jumping and loading. To set up the stack pointer, however, we need the *absolute physical* address.

The AArch64 instruction `adrp` loads the absolute address from a PC-relative offset within +/- 4 GiB. For AArch64, we simply get the absolute physical address and set the pointers:

```assembly
.global primary_core_boot
primary_core_boot:
// Load the stack address, set the stack and frame pointers.
  adrp    x0, __kernel_stack_start
  mov     sp, x0
  mov     fp, sp

// Halt
1:
  wfi
  b       1b
```

ARM, however, only has the `adr` instruction. It does the same thing as `ardp`, but it is limited to an offset within +/- 4 *KiB*. Assuming a page size of 4 KiB, you can just look at the linker script and see that the stack start is well beyond 4 KiB from the `.text` section.

We are going to use a little indirection trick. Consider:

```assembly
.global primary_core_boot
primary_core_boot:
	<some magic code goes here>
  mov     sp, r0
  mov     fp, sp

  ...

kernel_stack_start_rel:
	.word __kernel_stack_start - kernel_stack_start_rel
```

Immediately after `primary_core_boot`, we add a label, `kernel_stack_start_rel`, that is within 4 KiB of the boot code. At that label, we store the full 32-bit offset from that label to `__kernel_stack_start`. Now we can use `adr` to get the offset from the `pc` to `kernel_stack_start_rel`, the use `ldr` to load offset stored at that label, and finally add the two together to get the absolute physical address.

```assembly
.global primary_core_boot
primary_core_boot:
// Load the stack address, set the stack and frame pointers.
	adr     r0, kernel_stack_start_rel
	ldr     r1, kernel_stack_start_rel
	add     r0, r0, r1
  mov     sp, r0
  mov     fp, sp

// Halt
1:
  wfi
  b       1b

kernel_stack_start_rel:
	.word __kernel_stack_start - kernel_stack_start_rel
```

Voila! Propeller’s [layout.s](https://github.com/slakpi/Propeller-Kernel/blob/main/src/arch/arm/start/layout.s) file for ARM contains a bunch of utility functions for getting absolute physical addresses of locations in the kernel image before the MMU is enabled.

## Calling Our First Rust Function

Now that we have a stack, let’s call our first Rust function: the `memset` intrinsic provided by `rustc`.

The intrinsic has the following signature:

```rust
fn memset( dest: usize, val: u8, len: usize )
```

Remember that both the ARM and AArch64 procedure call standards require that the first three integer arguments be placed in `r0` - `r2`. For Aarch64, the function call is:

```assembly
  adrp    x0, __bss_start
  mov     x1, #0
  ldr     x2, =__bss_size
  bl      memset
```

The `adrp` instruction gets the absolute physical address of the BSS (zero-initialized) area for the `dest` argument in `x0`. The `mov` instruction specifies 0 for the `val` argument in `x1`. The `ldr` instruction gets the value of `__bss_size` for the `size` argument in `x2`. Finally, `bl` performs a branch-and-link to `memset`.

At the very beginning of `_start`, we saved the ATAG/DTB blob address to `w19`, the first of the *callee*-saved registers. Our code has not touched `w19`, and we know from the procedure call standard that `memset` must restore the value of `w19` before returning.

Alright! Now we are in a position where we can start creating some helper functions to set up the MMU and get it enabled.

-----
[Part 6](https://slakpi.github.io/Propeller-Kernel/part_6.html)
