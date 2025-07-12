//! Architecture-specific Module

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

pub mod common;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;
