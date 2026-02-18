# THE PROPELLER KERNEL - PART 1

## Introduction

So, you want to explore writing your own operating system and you want to do it in Rust? Cool, so did I. If you have not read it already, I will point you to OSDev's list of [Beginner Mistakes](https://wiki.osdev.org/Beginner_Mistakes). OSDev stresses the right things in that article: approach this with some achievable goals (learn about ARM system architecture, learn about Rust, etc.). Approach it with a desire to learn the *right* way to do things. *Do not* approach this with the initial goal of writing a complete operating system.

> **NOTE**: I am *not* an expert in kernel design. I am trying to learn the right ways myself, so it is likely that some of the things I do in the tutorial are not the best way. Treat this tutorial accordingly. Be suspicious. If you find a better way to do something, [let me know](mailto://randy.widell@gmail.com)!

I started by translating Raspberry Pi tutorials written in C to Rust, then expanding on them and cleaning up the code as I learned. That initial project, [ROS](https://github.com/slakpi/ros), quickly went off the rails for a variety of reasons.

When JetBrains made RustRover freely available for non-commercial use, I decided to start over with Propeller. I had two main goals:

* Go Cargo-native by removing [CMake](https://cmake.org/) and the [ARM GNU toolchain](https://developer.arm.com/downloads/-/arm-gnu-toolchain-downloads).
* Enforce a rule on myself that all changes to the kernel source must be tested in QEMU and on hardware before I can push to `main`. As such, Propeller's `main` branch has cleaner commits and always works.

The switch from the CMake + [Corrosion](https://github.com/corrosion-rs/corrosion) + Cargo setup in ROS to just Cargo was not super easy, and I still needed the ARM GNU toolchain for its assembler and debugger. But, I think it worked out nicely enough to put together a coherent tutorial on building a toy kernel with RustRover, the ARM GNU toolchain, Python for some build tooling.

This tutorial's inclusion of 32-bit ARM is a bit novel from what I have seen of toy kernel tutorials around the Internet. Relatively speaking, 64-bit is easy. In a world where 64-bit systems exist, 32-bit seems weird and hacky when you start learning about how virtual address spaces work.

I decided to walk both paths for the simple pleasure of learning how 32-bit kernels dealt with the address space limitations. It was a good choice that led to some really interesting history lessons about the evolution of how Windows and Linux handled it.

## Resources

There are a ton of existing tutorials, and I will give them credit in time. I do not want to mention them up front, because I want you to start fresh. I will credit them when I mention specific things I learned from them.

### JetBrains RustRover

[RustRover](https://www.jetbrains.com/rust/download).

### ARM GNU Toolchain

Download the [ARM GNU toolchain](https://developer.arm.com/downloads/-/arm-gnu-toolchain-downloads). You want the `arm-none-eabi` and `aarch64-none-elf` variants for your host system. They will be used to compile the assembly sources and provide target-specific builds of GDB for debugging.

### ARM Documents for Reference

* [ARM Cortex-A Series Programmer's Guide for ARMv7-A](https://developer.arm.com/documentation/den0013/0400/)
* [ARM Architecture Reference Manual for ARMv7-A and -R](https://developer.arm.com/documentation/ddi0406/latest)
* [ARM Architecture Reference Manual for ARMv8-A and ARMv9-A](https://developer.arm.com/documentation/ddi0487/latest)

### Python

Make sure you have a [Python](https://www.python.org/downloads/) distribution on your machine for build tooling. Anything above Python 3.9 should be fine.

### QEMU

Download [QEMU](https://www.qemu.org/download/) for initial testing. QEMU's default Raspberry Pi profiles work well. We will use the Pi 2 profile for 32-bit and the Pi 3 profile for 64-bit.

### Raspberry Pi

If you want to try running your operating system on hardware, buy a cheap Raspberry Pi. A [Raspberry Pi 3B](https://www.adafruit.com/product/3055) is great. It is $35 and it has a Cortex-A53 that can support both AArch64 and ARMv7 kernels. A Raspberry Pi 4 is fine, just do not go crazy on RAM. Remember: a 32-bit operating system can only support up to 3 GiB of physical RAM. 1 GiB is fine and plenty enough to make 32-bit interesting.

### JTAG

I highly recommend buying a JTAG debugger if you want to try running on hardware. Testing in QEMU does not guarantee things will go smoothly on hardware, and you will be left guessing why if the kernel panics early. I have been there, and done that.

I bought the [FTDI FT4232H Mini](https://www.mouser.com/ProductDetail/FTDI/FT4232H-MINI-MODULE?qs=y8i7Sk8A7hKBoUzIUiNYjg%3D%3D). It took some work to get it set up, but I remembered to thoroughly document the setup and will include that information later. The FTDI module can receive from the UART as well. This means you can see output in a serial terminal while debugging.

### Serial Terminal

My home machine is a MacBook Pro, so I went with [CoolTerm](https://freeware.the-meiers.org/) which also has Windows and Linux builds. CoolTerm works great, is still actively developed, and is free. But, be cool, leave Roger a donation if you use CoolTerm and like it.

### Books and Such

I bought a book on [Linux kernel programming](https://www.packtpub.com/en-us/product/linux-kernel-programming-9781803241081) and sought out the [Windows Research Kernel](https://github.com/HighSchoolSoftwareClub/Windows-Research-Kernel-WRK-) to use as touchstones.

> **CAUTION:** I am not sure it was legal for HighSchoolSoftwareClub to post the Windows Research Kernel on GitHub. But, it has been there for 8 years and it is not lurking in any shadows. Use at your own risk.

The fun of this project is to try to solve problems first, then look at how Windows and Linux did it. It is really exciting when you think of a way to do something and find out they do it the same way!

The [Embedded Rust Book](https://docs.rust-embedded.org/book/index.html) has a lot of good information about writing Rust for bare metal systems.

## Why Raspberry Pi?

The Raspberry Pi, aside from being a cheap, fully capable computer, has two nice advantages:

* It already has a boot loader that adheres to the [ARM](https://www.kernel.org/doc/Documentation/arm/booting.rst) and [AArch64](https://www.kernel.org/doc/Documentation/arm64/booting.txt) Linux boot protocols.
* The boot loader only needs to you to drop the kernel image on a SD card with the right name.

So easy!

Once you get going and feel like you might want to do an x86[_64] port as well, [Writing an OS in Rust](https://os.phil-opp.com/) by Philipp Oppermann can get you started. He actually seems to know what he is doing and even has an x86_64 boot loader you can use.

-----
[Part 2](https://slakpi.github.io/Propeller-Kernel/part_2.html)
