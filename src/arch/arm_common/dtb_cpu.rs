//! ARM Common DTB CPU Scanner

use super::cpu::{self, Core, CoreConfig, CoreEnableMethod};
use crate::support::{dtb, hash, hash_map};
use core::cmp;

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
  primary_id: usize,
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
    let mut map = StringMap::new(hash::BuildFnv1aHasher {});

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
  pub fn new(config: &'config mut CoreConfig, primary_id: usize) -> Self {
    DtbCoreScanner {
      config,
      primary_id,
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
          // For ARMv7, the thread ID is in bits [23:0] of MPIDR. For AArch64,
          // the thread ID can be either bits [23:0] of MPIDR_EL1 if the address
          // cell count is 1, or bits [39:32,23:0] if the address cell count is
          // 2. In all cases, the core ID will fit in a usize for the platform.
          core.id =
            Self::read_thread_id(header.size, self.addr_cells, reader, &mut tmp_cursor)? as usize;
        }

        _ => reader.skip_and_align(header.size, &mut tmp_cursor),
      }
    }

    let is_primary = core.id == self.primary_id;

    // Reserve a spot in the configuration to ensure that we always add the
    // primary core.
    if !is_primary && self.config.get_core_count() > cpu::MAX_CORES - 1 {
      return Ok(());
    }

    // Use the default enable method if this core does not specify one.
    match core.enable_method {
      CoreEnableMethod::Invalid => core.enable_method = self.def_enable_method,
      _ => {}
    }

    // Do not worry if we were unable to add the core. If there are too many
    // cores, we will just ignore it.
    _ = self.config.add_core(core, is_primary);

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
  /// For ARM, the thread ID may include the 2nd, 3rd, and 4th (AArch64)
  /// affinity levels. For example, Linux requires:
  ///
  /// * ARM - `reg` contains MPIDR bits [23:0]
  /// * AArch64 - `reg` contains MPIDR_EL1 bits [23:0]. If address cells is 2,
  ///   the second word contains MPIDR_EL1 bits [39:32].
  ///
  /// https://www.kernel.org/doc/Documentation/devicetree/bindings/arm/cpus.txt
  ///
  /// # Assumptions
  ///
  /// Assumes one thread per core.
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

/// Get the core configuration.
///
/// # Parameters
///
/// * `config` - The core configuration.
/// * `blob_vaddr` - The DTB virtual address.
///
/// # Assumptions
///
/// Assumes the caller is on the primary core.
///
/// # Returns
///
/// True if able to read the core configuration and at least one valid core is
/// provided by the DTB, false otherwise.
pub fn get_core_config(config: &mut CoreConfig, blob_vaddr: usize) -> bool {
  config.reset();

  let mut scanner = DtbCoreScanner::new(config, cpu::get_id());

  let reader = match dtb::DtbReader::new(blob_vaddr) {
    Ok(r) => r,
    _ => return false,
  };

  if !reader.scan(&mut scanner).is_ok() {
    return false;
  }

  // Validate that we have at least one core.
  if config.get_core_count() == 0 {
    return false;
  }

  // Validate that the enable method for each core is supported.
  for core in config.get_cores() {
    match core.enable_method {
      CoreEnableMethod::Invalid => return false,
      _ => {}
    }
  }

  true
}
