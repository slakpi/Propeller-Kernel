//! Architecture-specific Module

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "arm")]
mod arm;
#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
mod arm_common;
mod common;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
#[cfg(target_arch = "arm")]
pub use arm::*;
pub use common::bits;
