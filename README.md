# PROPELLER KERNEL

## Introduction

Propeller is a cleanup of my [ROS](https://github.com/slakpi/ros) project. ROS made it to a point where it was too polluted by bad ideas from other toy kernel projects, the commits where stream of conscious rather than deliberate, and it no longer ran on hardware. I was also very unsatisfied with using CMake + Corrosion + Cargo.

Propeller *is still very much a toy kernel*, and it does share some code with ROS. However:

* Propeller benefits from an improved understanding of the ARM and AArch64 architectures.
* Changes to Propeller are tested in 32- and 64-bit builds on ARMv8 hardware as well as in 32-bit ARMv7 and 64-bit ARMv8 QEMU runs before they make it to main. This means the commits are more deliberate rather than stream of conscious and main always works.
* Propeller uses only Cargo for building. However, it does also use a Python runner that creates a kernel image with `rust-objcopy` and an assembly listing with `rust-objdump`.

## Tutorial

Refer to the full [Tutorial](https://slakpi.github.io/Propeller-Kernel/part_1.html) for detailed information about Propeller and the background of design decisions.

## Architecture

Refer to the [Architecture](doc/ARCHITECTURE.md) summary document for details on the kernel design.

## Building

### Setup the GNU ARM Toolchains

1. Download the [GNU ARM Toolchains](https://developer.arm.com/downloads/-/arm-gnu-toolchain-downloads) for your platform. 32-bit ARMv7 builds require the `arm-none-eabi` toolchain, AArch64 builds require the `aarch64-none-elf` toolchain.
2. Install the toolchains somewhere. This document will use `/opt/cross` as an example.
3. Add the C compiler and archiver to your user's Cargo configuration, `$HOME/.cargo/config.toml`:


    [env]
    CC_aarch64_unknown_none_softfloat = "/opt/cross/gnu-aarch64-none-elf/bin/aarch64-none-elf-gcc"
    AR_aarch64_unknown_none_softfloat = "/opt/cross/gnu-aarch64-none-elf/bin/aarch64-none-elf-ar"
    CC_armv7a_none_eabi = "/opt/cross/gnu-arm-none-eabi/bin/arm-none-eabi-gcc"
    AR_armv7a_none_eabi = "/opt/cross/gnu-arm-none-eabi/bin/arm-none-eabi-ar"

4. Install Python 3.9+ in a virtual environment at the root of the repo. The `config.toml` file in the repo's `.config` folder will use `.venv/bin/python` to invoke the post-build script.

### Install `llvm-tools` and `cargo-binutils`.

1. Use `rustup component add llvm-tools` to install the LLVM tools.
2. Use `cargo install cargo-binutils` to add the Cargo commands for `objcopy` and `objdump`.

### Install the Rust Toolchains

1. Use `rustup toolchain install armv7a-none-eabi` to install the 32-bit ARMv7 toolchain.
2. Use `rustup toolchain install aarch64-unknown-none-softfloat` to install the AArch64 toolchain.

### 32-bit Cortex-A7 (Raspberry Pi 2) QEMU Build

QEMU expects the kernel to start at 0x10000 for Raspberry Pi 2.

    cargo run --config .cargo/config-qemu.toml --target armv7a-none-eabi -- --image kernel7.img

### 32-bit Cortex-A53 (Raspberry Pi 3) Hardware Build

    cargo run --config .cargo/config-rpi3.toml --target armv7a-none-eabi -- --image kernel7.img

### 64-bit Cortex-A53 (Raspberry Pi 3) QEMU Build

    cargo run --config .cargo/config-qemu.toml --target aarch64-unknown-none-softfloat -- --image kernel8.img

### 64-bit Cortex-A53 (Raspberry Pi 3) Hardware Build

    cargo run --config .cargo/config-rpi3.toml --target aarch64-unknown-none-softfloat -- --image kernel8.img

### 64-bit Cortex-A72 (Raspberry Pi 4) Hardware Build

    cargo run --config .cargo/config-rpi4.toml --target aarch64-unknown-none-softfloat -- --image kernel8.img

## QEMU Debugging

I copied the DTB files off of a SD card with a clean install of Raspberry Pi OS for debugging.

Invoke ARM QEMU with:

    qemu-system-arm -M raspi2b -kernel target/armv7a-none-eabi/debug/kernel7.img -dtb <path to DTBs>/bcm2709-rpi-2-b.dtb -serial null -serial stdio -gdb tcp::9000 -S

Invoke AArch64 QEMU with:

    qemu-system-aarch64 -M raspi3b -kernel target/aarch64-unknown-none-softfloat/debug/kernel8.img -dtb <path to DTBs>/bcm2710-rpi-3-b.dtb -serial null -serial stdio -gdb tcp::9000 -S

These commands will start QEMU in a halted state waiting for GDB connect. The repo provides GDB scripts to make debug setup a little faster. In another terminal:

Invoke ARM GDB:

    /opt/cross/gnu-arm-none-eabi/bin/arm-none-eabi-gdb -x support/debug/qemu/armv7a.gdb

Invoke AArch64 GDB:

    /opt/cross/gnu-aarch64-none-elf/bin/aarch64-none-elf-gdb -x support/debug/qemu/aarch64.gdb

## Hardware Debugging

Refer to the [OpenOCD README](support/debug/openocd/README.md).
