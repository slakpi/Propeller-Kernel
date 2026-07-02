//! ARM Common Debug Printing

/// Import one, and only one, serial debug output driver.
#[cfg(feature = "bcm2835_mini_uart_debug")]
mod bcm2835_mini_uart_debug;
#[cfg(feature = "bcm2835_pl011_uart_debug")]
mod bcm2835_pl011_uart_debug;

/// Import one, and only one, serial debug output interface.
#[cfg(feature = "bcm2835_mini_uart_debug")]
pub use bcm2835_mini_uart_debug::*;
#[cfg(feature = "bcm2835_pl011_uart_debug")]
pub use bcm2835_pl011_uart_debug::*;
