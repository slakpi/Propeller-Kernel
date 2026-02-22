//! Propeller Rustland Entry Point.

#![no_std]
#![no_main]
// When debug assertions are enabled (i.e. this is a debug build), allow unused
// variable and code.
#![cfg_attr(debug_assertions, allow(unused))]

mod arch;
mod mm;
mod support;
mod sync;
mod task;
#[cfg(feature = "module_tests")]
mod test;

use arch::memory::MemoryZone;
use core::ops::DerefMut;
use core::panic::PanicInfo;

/// Panic handler.
///
/// # Parameters
///
/// * `info` - Information about the panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
  arch::cpu::halt();
}

/// Single-threaded kernel initialization.
///
/// # Parameters
///
/// * `config` - Pointer to the architecture configuration struct.
#[unsafe(no_mangle)]
extern "C" fn pk_init(config: usize) {
  // Single-threaded initialization.
  arch::init(config);
  task::init();
  mm::init();

  // Run module tests single-threaded.
  #[cfg(feature = "module_tests")]
  run_module_tests();

  // Bring up any secondary cores.
  let mut alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_mut()
    .unwrap();
  arch::init_smp(alloc.lock().deref_mut());
}

/// Scheduler entry point.
#[unsafe(no_mangle)]
extern "C" fn pk_scheduler() -> ! {
  arch::cpu::halt();
}

#[cfg(feature = "module_tests")]
fn run_module_tests() {
  debug_print!("--- Running Module Tests ---\n");
  arch::run_tests();
  mm::run_tests();
  support::bits::run_tests();
}
