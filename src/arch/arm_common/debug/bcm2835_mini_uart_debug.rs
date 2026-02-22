//! BCM2835 Mini-UART Serial Debug Output Driver
//!
//! A low-level serial debug output driver that assumes a BCM2835-compatible
//! SoC. The kernel must map the physical range provided by
//! `get_physical_range()` into the kernel's address space and provide the base
//! virtual address of the range to `init()`.
//!
//! The driver assumes the bootloader has configured the mini-UART. For example,
//! on a Raspberry Pi platform, this can be done by adding the following to the
//! config.txt file:
//!
//!   [all]
//!   enable_uart=1

use crate::sync::SpinLock;
use core::ptr;

/// BCM2835 auxiliary serial device and mini-UART registers.
const AUX_MU_IO_REG: usize = 0x1_5040;
const AUX_MU_LSR_REG: usize = 0x1_5054;

/// The base physical address of the BCM2835 GPIO and auxiliary serial device
/// registers.
const PHYSICAL_BASE_ADDRESS: usize = 0x3f20_0000;

/// The number of pages to map.
const PHYSICAL_SIZE: usize = 0x16000;

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// The base virtual address chosen by the kernel for the registers.
static mut VIRTUAL_BASE: usize = 0;

/// Serial port guard.
static mut DRIVER_LOCK: SpinLock<()> = SpinLock::new(());

/// Get the physical address range covered by this driver.
pub fn get_physical_range() -> (usize, usize) {
  (PHYSICAL_BASE_ADDRESS, PHYSICAL_SIZE)
}

/// Initialize the serial debug output driver.
///
/// # Parameters
///
/// * `virt_base` - The base virtual address for driver's memory range.
pub fn init(virt_base: usize) {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
    VIRTUAL_BASE = virt_base;
  }
}

/// Write a string to the serial debug output device.
///
/// # Parameter
///
/// * `s` - The string to write.
pub fn put_string(s: &str) {
  put_bytes(s.as_bytes());
}

/// Write bytes to the serial debug output device.
///
/// # Parameters
///
/// * `s` - The bytes to write.
pub fn put_bytes(s: &[u8]) {
  let guard = unsafe { ptr::addr_of_mut!(DRIVER_LOCK).as_mut().unwrap() }.lock();

  for c in s {
    loop {
      let c = reg_get(AUX_MU_LSR_REG);
      if c & 0x20 != 0 {
        break;
      }
    }

    reg_put(AUX_MU_IO_REG, *c as u32);
  }
}

/// Read a device register.
///
/// # Parameter
///
/// * `reg` - The device register to read.
///
/// # Returns
///
/// The value of the register.
fn reg_get(reg: usize) -> u32 {
  unsafe { ptr::read_volatile((VIRTUAL_BASE + reg) as *const u32) }
}

/// Write to a device register.
///
/// # Parameters
///
/// * `reg` - The device register to modify.
/// * `val` - The value to write.
fn reg_put(reg: usize, val: u32) {
  unsafe {
    ptr::write_volatile((VIRTUAL_BASE + reg) as *mut u32, val);
  }
}
