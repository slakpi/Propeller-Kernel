//! ARM Common Debug Printing

/// Import one, and only one, serial debug output driver.
#[cfg(any(feature = "bcm2835_mini_uart_debug", feature = "bcm2835_mini_uart_debug4"))]
mod bcm2835_mini_uart_debug;
#[cfg(feature = "bcm2835_pl011_uart_debug")]
mod bcm2835_pl011_uart_debug;

/// Import one, and only one, serial debug output interface.
#[cfg(any(feature = "bcm2835_mini_uart_debug", feature = "bcm2835_mini_uart_debug4"))]
pub use bcm2835_mini_uart_debug::*;
#[cfg(feature = "bcm2835_pl011_uart_debug")]
pub use bcm2835_pl011_uart_debug::*;
