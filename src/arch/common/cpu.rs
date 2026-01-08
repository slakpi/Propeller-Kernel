//! Common CPU Core Configuration Utilities

use crate::support::{hash, hash_map};

/// 32-bit builds are limited to 16 cores. Thread-local page mapping requires
/// each core to reserve a 2 MiB block in the kernel's address space. Limiting
/// the number of cores prevents the Thread Local area from overflowing into the
/// Low Memory area.
#[cfg(target_pointer_width = "64")]
pub const MAX_CORES: usize = 256;
#[cfg(target_pointer_width = "32")]
pub const MAX_CORES: usize = 16;

/// Length of a core type name.
pub const CORE_TYPE_LEN: usize = 64;

/// Size of the core ID to core index map, the smallest prime larger than 1.5x
/// the AArch64 max core count.
#[cfg(target_pointer_width = "64")]
pub const CORE_MAP_SIZE: usize = 389;
#[cfg(target_pointer_width = "32")]
pub const CORE_MAP_SIZE: usize = 29;

/// Method used to enable a core.
#[derive(Copy, Clone)]
pub enum CoreEnableMethod {
  /// Default invalid method.
  Invalid,
  /// Spin tables park each core in a loop watching a specific memory address. A
  /// core is enabled by writing the kernel start address to the watch address.
  SpinTable,
  /// BCM2836 is the Broadcom 2836 SoC mailbox enable method. It works the same
  /// way as the spin table method, but the watch addresses are defined in the
  /// Broadcom specification rather than the DeviceTree.
  Bcm2836,
}

/// Logical core information.
pub struct Core {
  pub id: usize,
  pub core_type: [u8; CORE_TYPE_LEN],
  pub enable_method: CoreEnableMethod,
  pub release_addr: usize,
}

impl Core {
  /// Construct an empty core.
  pub const fn new() -> Self {
    Self {
      id: 0,
      core_type: [0; CORE_TYPE_LEN],
      enable_method: CoreEnableMethod::Invalid,
      release_addr: 0,
    }
  }

  /// Get the core's ID.
  pub fn get_id(&self) -> usize {
    self.id
  }

  /// Get the core type byte string.
  pub fn get_core_type(&self) -> &[u8] {
    &self.core_type
  }

  /// Get the method to enable the core.
  pub fn get_enable_method(&self) -> CoreEnableMethod {
    self.enable_method
  }

  /// Get the address used for any of the enable methods that spin on an
  /// address.
  pub fn get_release_addr(&self) -> usize {
    self.release_addr
  }
}

/// Convenience type for mapping from a hardware core ID to an core index.
type IdMap = hash_map::HashMap<usize, usize, hash::BuildFnv1aHasher, CORE_MAP_SIZE>;

/// System logical core configuration.
pub struct CoreConfig {
  cores: [Core; MAX_CORES],
  core_count: usize,
  id_map: IdMap,
}

impl CoreConfig {
  const CORE_INITIALIZER: Core = Core::new();

  /// Construct a new core configuration.
  pub const fn new() -> Self {
    Self {
      cores: [Self::CORE_INITIALIZER; MAX_CORES],
      core_count: 0,
      id_map: IdMap::new(hash::BuildFnv1aHasher {}),
    }
  }

  /// Add a new core to the configuration.
  ///
  /// # Parameters
  ///
  /// * `core` - The core to add.
  ///
  /// # Returns
  ///
  /// True if able to add the core, false otherwise.
  pub fn add_core(&mut self, core: Core) -> bool {
    if self.core_count >= MAX_CORES {
      return false;
    }

    self.id_map.insert(core.id, self.core_count);
    self.cores[self.core_count] = core;
    self.core_count += 1;
    true
  }

  /// Reset the configuration.
  pub fn reset(&mut self) {
    self.core_count = 0;
  }

  /// Get the number of logical cores available.
  pub fn get_core_count(&self) -> usize {
    self.core_count
  }

  /// Get the core index from a physical core identifier.
  ///
  /// # Parameters
  ///
  /// * `id` - The physical core identifier.
  ///
  /// # Description
  ///
  /// There is no guarantee architecture-independent guarantee that physical
  /// core identifiers must be contiguous starting from zero. The ARM MPIDR
  /// register, for example, provides 24 (or 32) bit identifiers that may be
  /// hierarchical, but really have no set format.
  ///
  /// When discovering cores in a system, they can be added to the configuration
  /// in the order they are discovered to create a contiguous, zero-based index
  /// suitable for the kernel's data structures.
  ///
  /// This function provides a mapping from the physical identifier provided by
  /// the architecture's `cpu::get_id()` implementation to the core index used
  /// by the kernel.
  ///
  /// # Returns
  ///
  /// The index of the specified core.
  pub fn get_core_index(&self, id: usize) -> Option<usize> {
    // A linear search can be faster with a small number of cores.
    //
    // Time, in milliseconds, for 2^20 searches split equally among each of N
    // randomized 64-bit core IDs as profiled on a Raspberry Pi 3. The hash
    // table used the smallest prime larger than 1.5x the number of cores as the
    // size. The break-even point is between 32 and 48 cores on average. The
    // timings are similar for non-randomized core IDs.
    //
    //     | Cores | Naïve | Hash | Smart |
    //     |:------|:------|:-----|:------|
    //     | 4     | 15.4  | 68.1 | 15.6  |
    //     | 8     | 21.7  | 67.3 | 21.6  |
    //     | 16    | 34.4  | 78.4 | 34    |
    //     | 32    | 71.7  | 80.4 | 80.3  |
    //     | 48    | 98.9  | 93.3 | 95    |
    //     | 64    | 133.9 | 84.6 | 85.5  |
    //     | 96    | 196.8 | 92.5 | 93.2  |
    //     | 128   | 246   | 87.3 | 86.8  |
    //     | 192   | 365.7 | 93   | 94.1  |
    //     | 256   | 471.4 | 86.3 | 87.5  |
    if self.core_count <= 32 {
      for i in 0..self.core_count {
        if self.cores[i].id == id {
          return Some(i);
        }
      }

      return None;
    }

    if let Some(id) = self.id_map.find(id) {
      return Some(*id);
    }

    None
  }

  /// Get the list of cores.
  pub fn get_cores(&self) -> &[Core] {
    &self.cores[..self.core_count]
  }
}
