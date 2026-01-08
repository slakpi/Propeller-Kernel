//! Architecture-Dependent Bit Manipulation Utilities Wrapper

#[cfg(target_pointer_width = "32")]
mod bits32;
#[cfg(target_pointer_width = "64")]
mod bits64;

#[cfg(target_pointer_width = "32")]
pub use bits32::*;
#[cfg(target_pointer_width = "64")]
pub use bits64::*;
