//! ARM Common DTB Memory Scanner

use crate::arch::memory::{MemoryConfig, MemoryRange, MemoryRangeHandler, MemoryZone};
use crate::support::{dtb, hash, hash_map, range, range_set};
use core::cmp::{self, Ordering};

/// Tags for expected properties and values.
enum StringTag {
  DtbPropAddressCells,
  DtbPropSizeCells,
  DtbPropDeviceType,
  DtbPropReg,
}

type StringMap<'map> = hash_map::HashMap<&'map [u8], StringTag, hash::BuildFnv1aHasher, 31>;

/// Scans for DTB memory nodes.
struct DtbMemoryScanner<'mem> {
  config: &'mem mut MemoryConfig,
  handler: &'mem dyn MemoryRangeHandler,
  string_map: StringMap<'mem>,
  addr_cells: u32,
  size_cells: u32,
}

impl<'mem> DtbMemoryScanner<'mem> {
  /// Construct a new DTB memory scanner.
  ///
  /// # Parameters
  ///
  /// * `config` - The MemoryConfig that will store the ranges found in the DTB.
  /// * `handler` - The memory range handler.
  ///
  /// # Returns
  ///
  /// A new DtbMemoryScanner.
  pub fn new(config: &'mem mut MemoryConfig, handler: &'mem dyn MemoryRangeHandler) -> Self {
    DtbMemoryScanner {
      config,
      handler,
      string_map: Self::build_string_map(),
      addr_cells: 0,
      size_cells: 0,
    }
  }

  /// Build a string map for the scanner.
  ///
  /// # Returns
  ///
  /// A new string map for the expected properties and values.
  fn build_string_map() -> StringMap<'mem> {
    let mut string_map = StringMap::new(hash::BuildFnv1aHasher {});

    string_map.insert("#address-cells".as_bytes(), StringTag::DtbPropAddressCells);
    string_map.insert("#size-cells".as_bytes(), StringTag::DtbPropSizeCells);
    string_map.insert("device_type".as_bytes(), StringTag::DtbPropDeviceType);
    string_map.insert("reg".as_bytes(), StringTag::DtbPropReg);

    string_map
  }

  /// Reads the root cell configuration.
  ///
  /// # Parameters
  ///
  /// * `reader` - The DTB reader.
  /// * `cursor` - The cursor pointing to the root node.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the cell configuration, otherwise a DTB error.
  fn scan_root_node(
    &mut self,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let mut tmp_cursor = *cursor;

    while let Some(header) = reader.get_next_property(&mut tmp_cursor) {
      let tag = self.string_map.find(header.name);

      match tag {
        Some(StringTag::DtbPropAddressCells) => {
          self.addr_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;
        }

        Some(StringTag::DtbPropSizeCells) => {
          self.size_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;
        }

        _ => reader.skip_and_align(header.size, &mut tmp_cursor),
      }
    }

    Ok(())
  }

  /// Scans a device node. If the device is a memory device, the function adds
  /// the memory ranges to the memory layout.
  ///
  /// # Parameters
  ///
  /// * `reader` - The DTB reader.
  /// * `cursor` - The cursor pointing to the device node.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the device node, otherwise a DTB error.
  fn scan_device_node(
    &mut self,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let mut tmp_cursor = *cursor;

    // Save the position and size of the device type and reg properties to check
    // after reading all of the node's properties.
    let mut dev_type: Option<(dtb::DtbCursor, usize)> = None;
    let mut reg: Option<(dtb::DtbCursor, usize)> = None;

    // Use the root address and size cell counts by default, and let this node
    // override them.
    let mut addr_cells = self.addr_cells;
    let mut size_cells = self.size_cells;

    while let Some(header) = reader.get_next_property(&mut tmp_cursor) {
      let tag = self.string_map.find(header.name);

      match tag {
        Some(StringTag::DtbPropDeviceType) => dev_type = Some((tmp_cursor, header.size)),

        Some(StringTag::DtbPropReg) => reg = Some((tmp_cursor, header.size)),

        Some(StringTag::DtbPropAddressCells) => {
          addr_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;
          continue;
        }

        Some(StringTag::DtbPropSizeCells) => {
          size_cells = reader
            .get_u32(&mut tmp_cursor)
            .ok_or(dtb::DtbError::InvalidDtb)?;
          continue;
        }

        _ => {}
      }

      reader.skip_and_align(header.size, &mut tmp_cursor);
    }

    match dev_type {
      Some((pos, size)) => {
        if !self.check_device_type(size, reader, &pos) {
          return Ok(());
        }
      }

      _ => return Ok(()),
    }

    match reg {
      Some((pos, size)) => {
        self.add_memory_blocks(size, addr_cells, size_cells, reader, &pos);
        Ok(())
      }

      _ => Ok(()),
    }
  }

  /// Check for a memory device.
  ///
  /// # Parameters
  ///
  /// * `prop_size` - The size of the property value.
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Returns
  ///
  /// Returns true if the device is a memory device, false otherwise.
  fn check_device_type(
    &self,
    _prop_size: usize,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> bool {
    let mut tmp_cursor = *cursor;

    if let Some(name) = reader.get_null_terminated_u8_slice(&mut tmp_cursor) {
      return name.cmp(b"memory") == Ordering::Equal;
    }

    false
  }

  /// Read a memory register property of (base address, size) pairs and add them
  /// to the memory configuration.
  ///
  /// # Parameters
  ///
  /// * `prop_size` - The size of the register property.
  /// * `addr_cells` - The number of address cells.
  /// * `size_cells` - The number of size cells.
  /// * `reader` - The DTB reader.
  /// * `cursor` - The current position in the DTB.
  ///
  /// # Returns
  ///
  /// Returns Ok if able to read the register property, otherwise a DTB error.
  fn add_memory_blocks(
    &mut self,
    prop_size: usize,
    addr_cells: u32,
    size_cells: u32,
    reader: &dtb::DtbReader,
    cursor: &dtb::DtbCursor,
  ) -> Result<(), dtb::DtbError> {
    let pair_size = dtb::DtbReader::get_reg_pair_size(addr_cells, size_cells);
    let mut tmp_cursor = *cursor;

    // Sanity check the DTB.
    if (pair_size == 0)
      || (prop_size == 0)
      || (prop_size < pair_size)
      || (prop_size % pair_size != 0)
    {
      return Err(dtb::DtbError::InvalidDtb);
    }

    for _ in 0..(prop_size / pair_size) {
      let (base, size) = reader
        .get_reg_pair(addr_cells, size_cells, &mut tmp_cursor)
        .ok_or(dtb::DtbError::InvalidDtb)?;

      // The base is beyond the platform's addressable range, just skip it. This
      // really only applies to 32-bit platforms.
      if base > usize::MAX as u64 {
        continue;
      }

      // Skip zero-size ranges.
      if size == 0 {
        continue;
      }

      // u64::MAX is largest *size* that can be expressed in a DTB. That means
      // the largest *range* possible is〈0x0, u64::MAX〉which translates to the
      // interval [0x0, u64::MAX - 1].
      //
      // However,〈0x1, u64::MAX〉and〈u64::MAX, 1〉are also a *valid* ranges, so
      // we cannot just use `u64::MAX - base` to calculate the maximum size from
      // the base.
      //
      // We have already checked that the base is less than or equal to
      // usize::MAX, so subtracting the base to get the maximum size of the
      // range in the platform's addressable range is safe.
      let size = if base == 0 {
        cmp::min(size, usize::MAX as u64)
      } else {
        cmp::min(size, usize::MAX as u64 - base + 1)
      };

      self
        .handler
        .handle_range(self.config, base as usize, size as usize);
    }

    Ok(())
  }
}

impl<'mem> dtb::DtbScanner for DtbMemoryScanner<'mem> {
  /// See `dtb::DtbScanner::scan_node()`
  fn scan_node(
    &mut self,
    reader: &dtb::DtbReader,
    name: &[u8],
    cursor: &dtb::DtbCursor,
  ) -> Result<bool, dtb::DtbError> {
    if name.len() == 0 {
      _ = self.scan_root_node(reader, cursor)?;
    } else {
      _ = self.scan_device_node(reader, cursor)?;
    }

    Ok(true)
  }
}

/// Get the system memory layout.
///
/// # Parameters
///
/// * `config` - The memory configuration.
/// * `handler` - The memory range handler.
/// * `blob` - The DTB address.
///
/// # Assumptions
///
/// Assumes the configuration is empty.
///
/// # Returns
///
/// True if able to read the memory configuration and at least one valid memory
/// range is provided by the SoC, false otherwise.
pub fn get_memory_layout(
  config: &mut MemoryConfig,
  handler: &dyn MemoryRangeHandler,
  blob: usize,
) -> bool {
  debug_assert!(config.is_empty());

  let mut scanner = DtbMemoryScanner::new(config, handler);

  let reader = match dtb::DtbReader::new(blob) {
    Ok(r) => r,
    _ => return false,
  };

  if !reader.scan(&mut scanner).is_ok() {
    return false;
  }

  config.trim_ranges();

  if config.is_empty() {
    return false;
  }

  true
}
