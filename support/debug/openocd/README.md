# OpenOCD DEBUGGING

Interface, target, and GDB scripts to debug hardware with OpenOCD. Currently, only the FTDI FT4232H JTAG GDB server is supported, and only Raspberry Pi 3 and 4 hardware are supported.

## Connections

### Serial (GPIO Alt4 Configuration)

    USB Serial          Raspberry Pi
    --------------------------------
    Black (Ground)      6  (Ground)
    White (RX)          8  (UART TX)
    Green (TX)          10 (UART RX)

### JTAG (GPIO Alt4 Configuration)

    FT4232H             Raspberry Pi
    --------------------------------
    CN2:1 - CN2:11      -
    CN2:7               22 (TCK)
    CN2:8               37 (TDI)
    CN2:9               18 (TDO)
    CN2:10              13 (TMS)
    CN2:12              15 (TRST)
    CN3:1 - CN3:3       -
    CN3:4               9  (Ground)
    CN3:25              8  (UART TX)
    CN3:26              10 (UART RX)

Use `usbserial-FT7GUT2I2` for serial communication.

## Debugging

Plugin in the Raspberry Pi and start OpenOCD using a command such as:

    openocd -f support/debug/openocd/interface/ft4232h.cfg -f support/debug/openocd/target/rpi3_aarch64.cfg

NOTE: The AArch64 target configurations work for both ARM and AArch64. The command only needs to change when switching between Raspberry Pi 3 and Raspberry Pi 4 hardware.

Start the AArch64 debugger:

    /opt/cross/gnu-aarch64-none-elf/bin/gnu-aarch64-none-elf-gdb -x support/debug/openocd/aarch64.gdb

Or the ARM debugger:

    /opt/cross/gnu-arm-none-eabi/bin/arm-none-eabi-gdb -x support/debug/openocd/armv7a.gdb
