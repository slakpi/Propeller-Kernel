//! System Device Tree Utilities

use super::cpu::CoreConfig;
use super::memory::{MemoryConfig, MemoryZone};

/// System device tree.
///
/// # Description
///
/// The system device tree is an architecture-independent representation of the
/// devices in the system. This should not be confused with a DeviceTree blob.
pub struct DeviceTree {
  cores: CoreConfig,
  memory: MemoryConfig,
}

impl DeviceTree {
  /// Construct a new device tree.
  pub const fn new() -> Self {
    Self {
      cores: CoreConfig::new(),
      memory: MemoryConfig::new(MemoryZone::InvalidZone),
    }
  }

  /// Get a reference to the core configuration.
  pub fn get_core_config(&self) -> &CoreConfig {
    &self.cores
  }

  /// Get a mutable reference to the core configuration.
  pub fn get_core_config_mut(&mut self) -> &mut CoreConfig {
    &mut self.cores
  }

  /// Get a reference to the memory configuration.
  pub fn get_memory_config(&self) -> &MemoryConfig {
    &self.memory
  }

  /// Get a mutable reference to the memory configuration.
  pub fn get_memory_config_mut(&mut self) -> &mut MemoryConfig {
    &mut self.memory
  }
}
