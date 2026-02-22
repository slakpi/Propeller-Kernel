//! ARM Common Module
//!
//! The ARM common module houses architecture-independent, but ARM platform-
//! specific utilities.

pub mod cpu;
#[cfg(feature = "serial_debug_output")]
pub mod debug;
pub mod dtb_cpu;
pub mod dtb_memory;
pub mod sync;
