//! ARM Common CPU Utilities

use crate::support::{dtb, hash, hash_map};
use core::cmp;

unsafe extern "C" {
  fn cpu_halt() -> !;
  fn cpu_get_id() -> usize;
}

/// Maximum number of cores supported for an ARM SoC (see B4.1.106 MPIDR and
/// D17.2.101 MPIDR_EL1).
///
///   NOTE: ARM builds are limited to 16 cores. `map_page_local` requires each
///         core to reserve a 2 MiB block in the kernel's address space.
///         Limiting the number of cores prevents the Thread Local area from
///         overflowing into the low memory area.
#[cfg(target_arch = "aarch64")]
pub const MAX_CORES: usize = 256;
#[cfg(target_arch = "arm")]
pub const MAX_CORES: usize = 16;

/// Length of a core type name.
pub const CORE_TYPE_LEN: usize = 64;

/// Method used to enable a core.
///
/// # Methods
///
/// * Spin tables park each core in a loop watching a specific memory address. A
///   core is enabled by writing the kernel start address to the watch address.
///
/// * BCM2836 is the Broadcom 2836 SoC mailbox enable method. It works the same
///   way as the spin table method, but the watch addresses are defined in the
///   Broadcom specification rather than the DeviceTree.
#[derive(Copy, Clone)]
pub enum CoreEnableMethod {
  Invalid,
  SpinTable,
  Bcm2836,
}

/// Core information.
#[derive(Copy, Clone)]
pub struct Core {
  valid: bool,
  id: u64,
  core_type: [u8; CORE_TYPE_LEN],
  enable_method: CoreEnableMethod,
  release_addr: usize,
}

impl Core {
  /// Construct an empty core.
  pub const fn new() -> Self {
    Core {
      valid: false,
      id: 0,
      core_type: [0; CORE_TYPE_LEN],
      enable_method: CoreEnableMethod::Invalid,
      release_addr: 0,
    }
  }

  /// Is the core initialized?
  pub fn is_valid(&self) -> bool {
    self.valid
  }

  /// Get the core's ID.
  pub fn get_id(&self) -> u64 {
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

/// System core configuration.
pub type CoreConfig = [Core; MAX_CORES];

/// Tags for CPU properties and string values.
enum DtbStringTag {
  DtbPropAddressCells,
  DtbPropSizeCells,
  DtbPropCompatible,
  DtbPropEnableMethod,
  DtbPropCpuReleaseAddr,
  DtbPropReg,

  DtbValueSpinTable,
  DtbValueBcm2836,
}

type StringMap = hash_map::HashMap<&'static [u8], DtbStringTag, hash::BuildFnv1aHasher, 31>;

/// Core node scanner.
struct DtbCoreScanner<'config> {
  config: &'config mut CoreConfig,
  string_map: StringMap,
  addr_cells: u32,
  def_enable_method: CoreEnableMethod,
}

impl<'config> DtbCoreScanner<'config> {
  /// Build a string map for the scanner.
  ///
  /// # Returns
  ///
  /// A new string map for the expected properties and values.
  fn build_string_map() -> StringMap {
    let mut map = hash_map::HashMap::with_hasher_factory(hash::BuildFnv1aHasher {});

    map.insert("#address-cells".as_bytes(), DtbStringTag::DtbPropAddressCells);
    map.insert("#size-cells".as_bytes(), DtbStringTag::DtbPropSizeCells);
    map.insert("compatible".as_bytes(), DtbStringTag::DtbPropCompatible);
    map.insert("enable-method".as_bytes(), DtbStringTag::DtbPropEnableMethod);
    map.insert("cpu-release-addr".as_bytes(), DtbStringTag::DtbPropCpuReleaseAddr);
    map.insert("reg".as_bytes(), DtbStringTag::DtbPropReg);

    map.insert("spin-table".as_bytes(), DtbStringTag::DtbValueSpinTable);
    map.insert("brcm,bcm2836-smp".as_bytes(), DtbStringTag::DtbValueBcm2836);

    map
  }

  /// Construct a new DtbCoreScanner.
  pub fn new(config: &'config mut CoreConfig) -> Self {
    DtbCoreScanner {
      config,
      string_map: Self::build_string_map(),
      addr_cells: 0,
      def_enable_method: CoreEnableMethod::Invalid,
    }
  }

  /// Scan the `cpus` node for a default core enable method.
  ///
  /// # Parameters
  ///
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Description
  ///
  /// The DeviceTree may specify a default core enable method in the `cpus` node
  /// for all cores, or it may define a core enable method in each `cpu@N` node.
  /// If a `cpu@N` node does not specify a core enable method, the method found
  /// here will be used.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the node, otherwise a DTB error.
  fn scan_cpus_node(
    &mut self,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let mut tmp_cursor = *cursor;

    while let Some(header) = reader.get_next_property(&mut tmp_cursor) {
      match self.string_map.find(header.name) {
        Some(DtbStringTag::DtbPropAddressCells) => {
          self.addr_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;
        }

        Some(DtbStringTag::DtbPropSizeCells) => {
          let size_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;

          if size_cells != 0 {
            return Err(dtb::DtbError::InvalidDtb);
          }
        }

        Some(DtbStringTag::DtbPropEnableMethod) => {
          self.def_enable_method =
            Self::read_enable_method(reader, &mut tmp_cursor, &self.string_map)?
        }

        _ => reader.skip_and_align(header.size, &mut tmp_cursor),
      }
    }

    // We need at least one address cell to read the thread identifiers.
    if self.addr_cells == 0 {
      return Err(dtb::DtbError::InvalidDtb);
    }

    Ok(())
  }

  /// Scan a `cpu@N` node and add it to the set of known cores.
  ///
  /// # Parameters
  ///
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the node, otherwise a DTB error.
  fn scan_cpu_node(
    &mut self,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let mut tmp_cursor = *cursor;
    let mut core = Core::new();

    while let Some(header) = reader.get_next_property(&mut tmp_cursor) {
      match self.string_map.find(header.name) {
        Some(DtbStringTag::DtbPropCompatible) => {
          Self::read_compatible(&mut core.core_type, reader, &mut tmp_cursor)?;
        }

        Some(DtbStringTag::DtbPropEnableMethod) => {
          core.enable_method = Self::read_enable_method(reader, &mut tmp_cursor, &self.string_map)?
        }

        Some(DtbStringTag::DtbPropCpuReleaseAddr) => {
          core.release_addr = Self::read_cpu_release_addr(header.size, reader, &mut tmp_cursor)?
        }

        Some(DtbStringTag::DtbPropReg) => {
          core.id = Self::read_thread_id(header.size, self.addr_cells, reader, &mut tmp_cursor)?;
        }

        _ => reader.skip_and_align(header.size, &mut tmp_cursor),
      }
    }

    // For ARM and AArch64, bits [7:0] (Affinity0), are the core number on this
    // node.
    let core_num = (core.id & 0xff) as usize;

    // If there are more cores than we can handle, just ignore this core.
    if core_num >= MAX_CORES {
      return Ok(());
    }

    // Use the default enable method if this core does not specify one.
    match core.enable_method {
      CoreEnableMethod::Invalid => core.enable_method = self.def_enable_method,
      _ => {}
    }

    core.valid = true;
    self.config[core_num] = core;

    Ok(())
  }

  /// Read the `compatible` property with the core name.
  ///
  /// # Parameters
  ///
  /// * `core_type` - The slice to receive the string.
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the property, otherwise a DTB error.
  fn read_compatible(
    core_type: &mut [u8],
    reader: &dtb::DtbReader,
    cursor: &mut dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let compatible = reader
      .get_null_terminated_u8_slice(cursor)
      .ok_or(dtb::DtbError::InvalidDtb)?;
    reader.skip_and_align(1, cursor);

    let len = cmp::min(compatible.len(), core_type.len());
    core_type[..len].clone_from_slice(&compatible[..len]);

    Ok(())
  }

  /// Read the `enable-method` property.
  ///
  /// # Parameters
  ///
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Returns
  ///
  /// Returns Ok with the enable method if valid, otherwise a DTB error.
  fn read_enable_method(
    reader: &dtb::DtbReader,
    cursor: &mut dtb::DtbCursor,
    string_map: &StringMap,
  ) -> Result<CoreEnableMethod, dtb::DtbError> {
    let enable_method = reader
      .get_null_terminated_u8_slice(cursor)
      .ok_or(dtb::DtbError::InvalidDtb)?;
    reader.skip_and_align(1, cursor);

    let tag = string_map
      .find(&enable_method)
      .ok_or(dtb::DtbError::UnknownValue)?;

    match tag {
      DtbStringTag::DtbValueSpinTable => Ok(CoreEnableMethod::SpinTable),
      DtbStringTag::DtbValueBcm2836 => Ok(CoreEnableMethod::Bcm2836),
      _ => Err(dtb::DtbError::UnsupportedValue),
    }
  }

  /// Read the `cpu-release-addr` property.
  ///
  /// # Parameters
  ///
  /// * `size` - The size of the property's value.
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Description
  ///
  ///   NOTE: The `cpu-release-addr` property SHOULD always be 64-bit, however
  ///         there exist DTBs that use 32-bit addresses.
  ///         https://devicetree-specification.readthedocs.io/en/stable/devicenodes.html#cpus-cpu-node-properties
  ///
  /// # Returns
  ///
  /// Returns Ok with the core release address if valid, otherwise a DTB error.
  fn read_cpu_release_addr(
    size: usize,
    reader: &dtb::DtbReader,
    cursor: &mut dtb::DtbCursor,
  ) -> Result<usize, dtb::DtbError> {
    match size {
      4 => Ok(reader.get_u32(cursor).ok_or(dtb::DtbError::InvalidDtb)? as usize),

      8 => {
        let addr = reader.get_u64(cursor).ok_or(dtb::DtbError::InvalidDtb)?;
        usize::try_from(addr).or(Err(dtb::DtbError::InvalidDtb))
      }

      _ => Err(dtb::DtbError::UnsupportedValue),
    }
  }

  /// Read the `reg` property with the core number.
  ///
  /// # Parameters
  ///
  /// * `size` - The size of the property's value.
  /// * `addr_cells` - Address cell count.
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Description
  ///
  /// The `reg` property is an array of thread identifiers for each hardware
  /// thread supported by the core.
  ///
  ///   NOTE: Assumes one thread per core.
  ///
  /// For ARM, the thread ID may include the 2nd, 3rd, and 4th (AArch64)
  /// affinity levels. For example, Linux requires:
  ///
  /// * ARM - `reg` contains MPIDR bits [23:0]
  /// * AArch64 - `reg` contains MPIDR_EL1 bits [23:0]. If address cells is 2,
  ///   the second word contains MPIDR_EL1 bits [39:32].
  ///
  /// https://www.kernel.org/doc/Documentation/devicetree/bindings/arm/cpus.txt
  ///
  /// # Returns
  ///
  /// Returns Ok with the core number if valid, otherwise a DTB error.
  fn read_thread_id(
    size: usize,
    addr_cells: u32,
    reader: &dtb::DtbReader,
    cursor: &mut dtb::DtbCursor,
  ) -> Result<u64, dtb::DtbError> {
    let mut tmp_cursor = *cursor;
    let count = size / dtb::DtbReader::get_reg_pair_size(addr_cells, 0);
    let pair = reader
      .get_reg_pair(addr_cells, 0, &mut tmp_cursor)
      .ok_or(dtb::DtbError::InvalidDtb)?;
    Ok(pair.0)
  }
}

impl<'config> dtb::DtbScanner for DtbCoreScanner<'config> {
  /// See `dtb::DtbScanner::scan_node()`.
  fn scan_node(
    &mut self,
    reader: &dtb::DtbReader,
    name: &[u8],
    cursor: &dtb::DtbCursor,
  ) -> Result<bool, dtb::DtbError> {
    if name.cmp(b"cpus") == cmp::Ordering::Equal {
      _ = self.scan_cpus_node(reader, cursor)?;
    } else if name.len() >= 5 && name[..4].cmp(b"cpu@") == cmp::Ordering::Equal {
      _ = self.scan_cpu_node(reader, cursor)?;
    }

    Ok(true)
  }
}

/// Halt the caller.
pub fn halt() -> ! {
  unsafe { cpu_halt() };
}

/// Get the current core ID.
pub fn get_id() -> usize {
  unsafe { cpu_get_id() }
}

/// Get the core configuration.
///
/// # Parameters
///
/// * `config` - The core configuration.
/// * `blob_vaddr` - The DTB virtual address.
///
/// # Returns
///
/// True if able to read the core configuration and at least one valid core is
/// provided by the system, false otherwise.
pub fn get_core_config(config: &mut CoreConfig, blob_vaddr: usize) -> bool {
  // Invalidate the whole list of cores. Explicit into_iter() to avoid move.
  for core in config.into_iter() {
    core.valid = false;
  }

  let mut scanner = DtbCoreScanner::new(config);

  let reader = match dtb::DtbReader::new(blob_vaddr) {
    Ok(r) => r,
    _ => return false,
  };

  if !reader.scan(&mut scanner).is_ok() {
    return false;
  }

  // Validate that we have at least one core and that the enable method for each
  // core is supported.
  let mut empty = true;

  for core in config {
    if !core.valid {
      continue;
    }

    empty = false;

    match core.enable_method {
      CoreEnableMethod::Invalid => return false,
      _ => {}
    }
  }

  if empty {
    return false;
  }

  true
}
