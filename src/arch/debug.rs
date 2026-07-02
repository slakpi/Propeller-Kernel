//! Architecture-Dependent Debug Utilities

#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
pub use super::arm_common::debug::*;

use crate::support::print;
use core::fmt::{Arguments, Write};

/// Formats the arguments to a string and writes it to the mini UART.
///
/// # Parameters
///
/// * `args` - The formatting arguments built by format_args!.
#[cfg(feature = "serial_debug_output")]
pub fn debug_print(args: Arguments) {
  const PRINT_BUFFER_SIZE: usize = 256;

  let mut buf = [0u8; PRINT_BUFFER_SIZE];
  let mut stream = print::WriteBuffer::new(&mut buf);
  match stream.write_fmt(args) {
    Ok(_) => put_bytes(stream.as_bytes()),
    _ => put_string("Error: debug_print Failed to format string.\n"),
  };
}
