//! Common Utilities Module
//!
//! The common utilities module houses architecture-dependent, but platform-
//! independent utilities.

#[cfg(target_pointer_width = "32")]
mod bits32;
#[cfg(target_pointer_width = "64")]
mod bits64;

pub mod bits;
