# THE PROPELLER KERNEL - PART 4

## Introduction

Hopefully you did not have any issues getting an ARM version of the kernel up and running. Let's assume all is well and try to run both versions in QEMU.

## QEMU

[QEMU](https://www.qemu.org/) is a great, cross-platform emulator that includes Raspberry Pi profiles we can use in place of Raspberry Pi hardware. Go ahead and make sure you have QEMU installed before reading on.

## Debug Profiles

QEMU supports remote debugging with GDB. Since we are going to be doing a lot of debugging, let's start by creating some GDB scripts that do some basic setup tasks to make our lives easier.

Create a file called `<propeller>/support/debug/qemu/aarch64.gdb` and add the following contents:

```
add-symbol-file target/aarch64-unknown-none-softfloat/debug/propeller
target remote localhost:9000
b *0x80000
```

Create another file called `<propeller>/support/debug/qemu/armv7a.gdb` and add the following contents:

```
add-symbol-file target/armv7a-none-eabi/debug/propeller
target remote :9000
b *0x10000
```

The first line in each file loads the symbols from the propeller binary so that we can get useful code listings when we actually have Rust code.

The second line connects to QEMU on port 9000.

The last line sets a breakpoint at the beginning of the kernel. Notice that we are using the physical address here.

## Starting QEMU

To start the AArch64 emulator, run:

```
$ qemu-system-aarch64 -M raspi3b -kernel target/aarch64-unknown-none-softfloat/debug/kernel8.img -gdb tcp::9000 -S
```

To start the ARMv7 emulator, run:

```
$ qemu-system-arm -M raspi2b -kernel target/armv7a-none-eabi/debug/kernel7.img -gdb tcp::9000 -S
```

The `-M raspi3b` and `-M raspi2b` options specify the Raspberry Pi hardware profile. The ARM emulator only includes a Raspberry Pi 2 profile, which is fine.

The `-kernel` option specifies the kernel image file to load.

The `-gdb tcp::9000` option tells QEMU to start a GDB debug server and listen at `localhost:9000`.

Finally, the `-S` option tells QEMU to immediately halt execution. This option allows you to connect GDB before anything happens. Once you connect GDB, you can let it run or step through the code.

## Starting GDB

After starting QEMU, open another terminal window and start GDB using the ARM toolchain that matches the emulator you started.

If you started AArch64 QEMU, run (replace `<AARCH64_PATH>` with the real path):

```
$ <AARCH64_PATH>/bin/aarch64-none-elf-gdb -x support/debug/qemu/aarch64.gdb
```

If you started ARM QEMU, run (replace `<ARM_PATH>` with the real path):

```
$ <ARM_PATH>/bin/arm-none-eabi-gdb -x support/debug/qemu/armv7a.gdb
```

## Using GDB

Let's use AArch64 as an example. After starting GDB, you should see something like:

```
0x0000000000000000 in ?? ()
Breakpoint 1 at 0x80000
(gdb)
```

If you use the `c` command to continue, GDB should let QEMU run and immediately hit the breakpoint at the beginning of the kernel.

```
(gdb) c
Continuing.

Thread 1 hit Breakpoint 1, 0x0000000000080000 in ?? ()
(gdb)
```

Are you excited yet? Your kernel is running!

Let's do a disassembly. ARM and AArch64 instructions are 32-bits, so let's use the `disassemble` command to disassemble the range 0x8_0000 - 0x8_0008:

```
(gdb) disassemble 0x80000,0x80008
Dump of assembler code from 0x80000 to 0x80008:
=> 0x0000000000080000:	wfi
   0x0000000000080004:	b	0x80000
End of assembler dump.
(gdb)
```

Hey! That's our infinite loop! How cool is that?

Using the `ni` command to step to the next instruction is just going to pause on wait-for-interrupt, so there is not a whole lot of fun we can have with our kernel. While we are just standing around here with nothing to do, however, why don't we...

## Just Poke at Things

Let's use the `info threads` command:

```
(gdb) info threads
  Id   Target Id                    Frame
* 1    Thread 1.1 (CPU#0 [running]) 0x0000000000080000 in ?? ()
  2    Thread 1.2 (CPU#1 [running]) 0x000000000000030c in ?? ()
  3    Thread 1.3 (CPU#2 [running]) 0x000000000000030c in ?? ()
  4    Thread 1.4 (CPU#3 [running]) 0x000000000000030c in ?? ()
(gdb)
```

Four hardware threads!? Yep. The Cortex-A7 and Cortex-A53 are quad-core, [symmetric multiprocessors](https://en.wikipedia.org/wiki/Symmetric_multiprocessing).

CPU#0 is currently stopped at 0x8_0000, which makes sense. But, what are CPU#1, 2, and 3 doing? It seems like there is a party at 0x30c that CPU#0 was not invited to!? Let's go over and check out what they are doing.

`t 2` will switch us to thread 2.

```
(gdb) t 2
[Switching to thread 2 (Thread 1.2)]
#0  0x000000000000030c in ?? ()
(gdb)
```

Let's disassemble the next...I don't know, choose a random number, say...3 instructions (12 bytes):

```
(gdb) disassemble 0x30c,0x318
Dump of assembler code from 0x30c to 0x318:
=> 0x000000000000030c:	wfe
   0x0000000000000310:	ldr	x4, [x5, x6, lsl #3]
   0x0000000000000314:	cbz	x4, 0x30c
End of assembler dump.
(gdb)
```

Huh, that's interesting. So, CPU#1, 2, and 3 are executing a wait-for-event, loading the value at some address, then looping back to the wait if the value is zero. Weird. Why would they do that?

Let's disassemble a few more instructions:

```
(gdb) disassemble 0x30c,0x32c
Dump of assembler code from 0x30c to 0x32c:
=> 0x000000000000030c:	wfe
   0x0000000000000310:	ldr	x4, [x5, x6, lsl #3]
   0x0000000000000314:	cbz	x4, 0x30c
   0x0000000000000318:	mov	x0, #0x0                   	// #0
   0x000000000000031c:	mov	x1, #0x0                   	// #0
   0x0000000000000320:	mov	x2, #0x0                   	// #0
   0x0000000000000324:	mov	x3, #0x0                   	// #0
   0x0000000000000328:	br	x4
End of assembler dump.
(gdb)
```

Interesting. If CPU#1, 2, and 3 see a value other than zero at the mysterious address, they zero out the argument registers and jump to the loaded address.

Let's step through the instructions:

```
(gdb) ni
0x0000000000000310 in ?? ()
(gdb) ni
0x0000000000000314 in ?? ()
(gdb) ni
0x000000000000030c in ?? ()
(gdb)
```

So, we looped back to 0x30c. The value at the address CPU#1 is examining must be zero.

What is that address anyway? Let's examine the registers with the `p` (print) command using hexadecimal (x) formatting:

```
(gdb) p/x $x5
$9 = 0xd8
(gdb) p/x $x6
$10 = 0x1
(gdb)
```

So, CPU#1 is trying to load from `0xd8 + 1 << 3 = 0xe0`. Let's check the other threads:

```
(gdb) t 3
[Switching to thread 3 (Thread 1.3)]
#0  0x000000000000030c in ?? ()
(gdb) p/x $x5
$11 = 0xd8
(gdb) p/x $x6
$12 = 0x2
(gdb) t 4
[Switching to thread 4 (Thread 1.4)]
#0  0x000000000000030c in ?? ()
(gdb) p/x $x5
$13 = 0xd8
(gdb) p/x $x6
$14 = 0x3
(gdb)
```

It looks like CPU#2 and 3 are reading from 0xe8 and 0xf0. Out of total curiosity, what if we write the kernel start address to 0xe0? Let's use the `set` command with a C-style cast and dereference to assign a value to the address. We can then use the `x` (examine) command to display 1 gigantic (g) word using hexadecimal (x).

```
(gdb) set *((int*)0xe0) = 0x80000
(gdb) x/1gx 0xe0
0xe0:	0x0000000000080000
(gdb)
```

Now let's step CPU#1 and see what happens.

```
(gdb) t 2
[Switching to thread 2 (Thread 1.2)]
#0  0x000000000000030c in ?? ()
(gdb) ni
0x0000000000000310 in ?? ()
(gdb) ni
0x0000000000000314 in ?? ()
(gdb) ni
0x0000000000000318 in ?? ()
(gdb) ni
0x000000000000031c in ?? ()
(gdb) ni
0x0000000000000320 in ?? ()
(gdb) ni
0x0000000000000324 in ?? ()
(gdb) ni
0x0000000000000328 in ?? ()
(gdb) ni

Thread 2 hit Breakpoint 1, 0x0000000000080000 in ?? ()
(gdb)
```

Woah! We just went multi-threaded! CPU#1 is now running the kernel as well!

## Parking Spaces

Not only did we learn some useful GDB commands, we also learned something very interesting about the state of the cores at boot!

Refer back to the Linux [AArch64](https://www.kernel.org/doc/Documentation/arm64/booting.txt) boot protocol. Linux requires that the boot loader "park" all but one core and disable all interrupts before jumping the "primary" core to kernel. The boot loader parks the "secondary" cores by putting them into a loop that checks a table for a jump address. Each core has its own entry in the table (the index value in $x6).

Where the table is located and how we go about bringing the cores up is a topic for much later.

For now, the key takeaway is that the boot loader is going to ensure that the kernel is single-threaded on boot. Only one core will be running and interrupts will be disabled. This gives us a chance to do initialization work without having to worry about synchronization.

-----
[Part 5](https://slakpi.github.io/Propeller-Kernel/part_5.html)
