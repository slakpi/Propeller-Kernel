//! ARM Common Debug Printing

/// Import one, and only one, serial debug output driver.
#[cfg(feature = "bcm2835_mini_uart_debug")]
mod bcm2835_mini_uart_debug;

/// Import one, and only one, serial debug output interface.
#[cfg(feature = "bcm2835_mini_uart_debug")]
pub use bcm2835_mini_uart_debug::*;

use crate::support::print;
use core::fmt::{self, Write};
use core::ptr;

const PRINT_BUFFER_SIZE: usize = 256;

/// Formats the arguments to a string and writes it to the mini UART.
///
/// # Parameters
///
/// * `args` - The formatting arguments built by format_args!.
#[cfg(feature = "serial_debug_output")]
pub fn debug_print(args: fmt::Arguments) {
  let mut buf = [0u8; PRINT_BUFFER_SIZE];
  let mut stream = print::WriteBuffer::new(&mut buf);
  match stream.write_fmt(args) {
    Ok(_) => put_bytes(stream.as_bytes()),
    _ => put_string("Error: debug_print Failed to format string.\n"),
  };
}
