//! Architecture-specific Module

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "arm")]
pub mod arm;
#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
pub mod arm_common;

pub mod common;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
#[cfg(target_arch = "arm")]
pub use arm::*;
#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
pub use arm_common::*;

pub use common::*;
