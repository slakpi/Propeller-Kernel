//! Propeller Rustland Entry Point.

#![no_std]
#![no_main]
// When debug assertions are enabled (i.e. this is a debug build), allow unused
// variable and code.
#![cfg_attr(debug_assertions, allow(unused))]

use core::panic::PanicInfo;

/// Panic handler.
///
/// # Parameters
///
/// * `info` - Information about the panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
  loop {}
}

/// Single-threaded kernel initialization.
///
/// # Parameters
///
/// * `config` - Pointer to the architecture configuration struct.
#[unsafe(no_mangle)]
extern "C" fn pk_init(_config: usize) {}

/// Scheduler entry point.
#[unsafe(no_mangle)]
extern "C" fn pk_scheduler() -> ! {
  loop {}
}
