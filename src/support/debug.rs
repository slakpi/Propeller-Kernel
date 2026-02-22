//! Kernel Debug Utilities

/// Formats a string with provided arguments and writes the formatted string to
/// the debug device.
#[cfg(feature = "serial_debug_output")]
#[macro_export]
macro_rules! debug_print {
  ($($arg:tt)*) => {{
    $crate::arch::debug::debug_print(format_args!($($arg)*));
  }}
}

/// Placeholder for builds without serial debug output enabled.
#[cfg(not(feature = "serial_debug_output"))]
#[macro_export]
macro_rules! debug_print {
  ($($arg:tt)*) => {{}};
}
