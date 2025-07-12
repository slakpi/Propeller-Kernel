//! Architecture-Dependent Bit Manipulation Utilities Wrapper

#[cfg(target_pointer_width = "32")]
pub use super::bits32::*;
#[cfg(target_pointer_width = "64")]
pub use super::bits64::*;
