//! Common Utilities Module

#[cfg(target_pointer_width = "32")]
mod bits32;
#[cfg(target_pointer_width = "64")]
mod bits64;

pub mod bits;
pub mod cpu;
