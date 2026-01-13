//! ARM Architecture

mod exceptions;
mod mm;

pub mod task;

pub use super::arm_common::{cpu, memory, sync};

use super::arm_common::{dtb_cpu, dtb_memory};
use super::common::table_allocator::LinearTableAllocator;
use crate::support::{bits, dtb, range};
use core::ptr;
use memory::{MappingStrategy, MemoryConfig, MemoryRange, MemoryRangeHandler, MemoryZone};

unsafe extern "C" {
  fn _secondary_start();
}

/// Propeller requires LPAE. With LPAE enabled, pages must be 4 KiB and sections
/// are 2 MiB at Level 2. LPAE page table entries are 8 bytes.
const PAGE_SIZE: usize = 4096;

const PAGE_SHIFT: usize = 12;

const PAGE_MASK: usize = PAGE_SIZE - 1;

const SECTION_SIZE: usize = 2 * 1024 * 1024;

const SECTION_SHIFT: usize = 21;

const SECTION_MASK: usize = SECTION_SIZE - 1;

const PAGE_TABLE_ENTRY_SIZE: usize = 8;

const PAGE_TABLE_ENTRY_SHIFT: usize = 3;

/// Reserve the upper 128 MiB of the kernel segment for the high memory area.
const HIGH_MEM_SIZE: usize = 128 * 1024 * 1024;

/// The base virtual address of the exception vectors.
const VECTORS_VIRTUAL_BASE: usize = 0xffff_0000;

/// The base virtual address of the recursive map area.
const RECURSIVE_MAP_AREA: usize = 0xffc0_0000;

/// The size of the virtual area reserved for the page directory.
const PAGE_DIRECTORY_SIZE: usize = 32 * 1024 * 1024;

/// The base virtual address of the page directory.
const PAGE_DIRECTORY_VIRTUAL_BASE: usize = VECTORS_VIRTUAL_BASE - PAGE_DIRECTORY_SIZE;

/// Basic kernel configuration provided by the start code. All address are
/// physical.
#[repr(C)]
#[derive(Copy, Clone)]
struct KernelConfig {
  virtual_base: usize,
  page_size: usize,
  blob: usize,
  kernel_base: usize,
  kernel_size: usize,
  kernel_pages_start: usize,
  kernel_pages_size: usize,
  vm_split: usize,
  kernel_stack_list: usize,
  kernel_stack_pages: usize,
  primary_stack_start: usize,
}

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// Kernel configuration provided by the start code.
static mut KERNEL_CONFIG: KernelConfig = KernelConfig {
  virtual_base: 0,
  page_size: 0,
  blob: 0,
  kernel_base: 0,
  kernel_size: 0,
  kernel_pages_start: 0,
  kernel_pages_size: 0,
  vm_split: 0,
  kernel_stack_list: 0,
  kernel_stack_pages: 0,
  primary_stack_start: 0,
};

/// CPU core configuration.
static mut CORE_CONFIG: cpu::CoreConfig = cpu::CoreConfig::new();

/// Memory layout configuration.
static mut MEMORY_CONFIG: MemoryConfig = MemoryConfig::new(MemoryZone::InvalidZone);

/// The base virtual address of the thread local mapping area.
static mut THREAD_LOCAL_VIRTUAL_BASE: usize = 0;

/// Tags memory ranges with the appropriate zone.
pub struct RangeZoneTagger {
  high_mem_base: usize,
}

impl RangeZoneTagger {
  /// Construct a new MemoryRangeHandler32Bit.
  ///
  /// # Parameters
  ///
  /// * `high_mem_base` - The physical base address of high memory.
  pub fn new(high_mem_base: usize) -> Self {
    Self { high_mem_base }
  }
}

impl MemoryRangeHandler for RangeZoneTagger {
  /// See `MemoryRangeHandler::handle_range()`.
  ///
  /// # Description
  ///
  /// Splits the provided range at the high memory base and tags the resulting
  /// range(s) as appropriate before adding them to the configuration.
  fn handle_range(&self, config: &mut MemoryConfig, base: usize, size: usize) {
    let range = MemoryRange {
      tag: MemoryZone::InvalidZone,
      base,
      size,
    };

    let (rl, rh) = range.split(self.high_mem_base).unwrap();

    if let Some(rl) = rl {
      config.insert_range(MemoryRange {
        tag: MemoryZone::LowMemoryZone,
        base: rl.base,
        size: rl.size,
      });
    }

    if let Some(rh) = rh {
      config.insert_range(MemoryRange {
        tag: MemoryZone::HighMemoryZone,
        base: rh.base,
        size: rh.size,
      });
    }
  }
}

/// ARM platform configuration.
///
/// # Parameters
///
/// * `config_addr` - The physical kernel configuration address provided by the
/// start code.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
///
///   NOTE: Requires 4 KiB pages.
///
///   NOTE: Requires the kernel stack page count to be a power of two.
///
///   NOTE: Requires the blob to be a DTB.
pub fn init(config_addr: usize) {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  assert_ne!(config_addr, 0);

  let kconfig = unsafe { &*(config_addr as *const KernelConfig) };

  // Require 4 KiB pages.
  assert_eq!(kconfig.page_size, PAGE_SIZE);

  // Require a power-of-2 page count for the kernel stack size.
  assert!(bits::is_power_of_2(kconfig.kernel_stack_pages));

  // Calculate the blob virtual address and get its size. There is no need to do
  // any real error checking on the size. The DTB reader will error check during
  // scans. However, we do require a DTB, so assert if the blob is not a valid
  // DTB.
  let blob_vaddr = kconfig.virtual_base + kconfig.blob;
  let blob_size = dtb::DtbReader::check_dtb(blob_vaddr).unwrap_or(0);
  assert_ne!(blob_size, 0);

  // Validate the VM split and virtual base.
  assert!(
    (kconfig.vm_split == 3 && kconfig.virtual_base == 0xc000_0000)
      || (kconfig.vm_split == 2 && kconfig.virtual_base == 0x8000_0000)
  );

  unsafe {
    KERNEL_CONFIG = *kconfig;
  }

  init_core_config(blob_vaddr);
  init_memory_config(blob_vaddr, blob_size);
  init_direct_map();
}

/// Get the size of a page.
pub const fn get_page_size() -> usize {
  PAGE_SIZE
}

/// Get the page shift.
pub const fn get_page_shift() -> usize {
  PAGE_SHIFT
}

/// Get the page alignment mask.
pub const fn get_page_mask() -> usize {
  PAGE_MASK
}

/// Get the size of a section.
pub const fn get_section_size() -> usize {
  SECTION_SIZE
}

/// Get the section shift.
pub const fn get_section_shift() -> usize {
  SECTION_SHIFT
}

/// Get the section alignment mask.
pub const fn get_section_mask() -> usize {
  SECTION_MASK
}

/// Get the size of page table entry.
pub const fn get_page_table_entry_size() -> usize {
  PAGE_TABLE_ENTRY_SIZE
}

/// Get the page table entry shift.
pub const fn get_page_table_entry_shift() -> usize {
  PAGE_TABLE_ENTRY_SHIFT
}

/// Get the kernel base address.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_kernel_base() -> usize {
  unsafe { KERNEL_CONFIG.kernel_base }
}

/// Get the kernel virtual base address.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_kernel_virtual_base() -> usize {
  unsafe { KERNEL_CONFIG.virtual_base }
}

/// Get the virtual base address of a page table that maps a given virtual
/// address.
///
/// # Parameters
///
/// * `virt_addr` - A virtual address in the kernel's address space.
///
/// # Description
///
/// The Level 2 page table that serves the upper 1 GiB of the kernel's address
/// space has a recursive mapping to allow editing of itself and any Level 3
/// table entries.
///
///   NOTE: In a 2/2 split, the lower 1 GiB of the kernel's address space will
///         always contain linear physical memory mappings and will never need
///         to be modified after boot.
///
/// # Returns
///
/// The virtual address of the page table that maps a given virtual address or
/// None if the given virtual address is not in the upper 1 GiB of the kernel's
/// address space.
pub fn get_page_virtual_address_for_virtual_address(virt_addr: usize) -> Option<usize> {
  // Only the upper 1 GiB of the kernel address space is served by the recursive
  // map area.
  if virt_addr < 0xc000_0000 {
    return None;
  }

  let index = (virt_addr - 0xc000_0000) / SECTION_SIZE;
  Some(RECURSIVE_MAP_AREA + (index << PAGE_SHIFT))
}

/// Get the base virtual address of the thread local area for the current core.
///
/// # Parameter
///
/// * `core_idx` - The current core index.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_thread_local_virtual_base(core_idx: usize) -> usize {
  assert!(core_idx < get_core_config().get_core_count());
  let offset = core_idx * get_section_size();
  unsafe { THREAD_LOCAL_VIRTUAL_BASE + offset }
}

/// Get the base physical address of the high memory area.
fn get_high_mem_base() -> usize {
  usize::MAX - get_kernel_virtual_base() - HIGH_MEM_SIZE + 1
}

/// Get the full core configuration.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_core_config() -> &'static cpu::CoreConfig {
  unsafe { ptr::addr_of!(CORE_CONFIG).as_ref().unwrap() }
}

/// Get the core index of the current core.
///
/// # Description
///
/// For any non-trivial use of the core index, interrupts must be disabled prior
/// to calling to prevent the task from moving to another core.
pub fn get_current_core_index() -> usize {
  get_core_config().get_core_index(cpu::get_id()).unwrap()
}

/// Get the memory layout configuration.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_memory_config() -> &'static MemoryConfig {
  unsafe { ptr::addr_of!(MEMORY_CONFIG).as_ref().unwrap() }
}

/// Get the kernel configuration.
///
/// # Description
///
///   NOTE: Private to the ARM architecture.
fn get_kernel_config() -> &'static KernelConfig {
  unsafe { ptr::addr_of!(KERNEL_CONFIG).as_ref().unwrap() }
}

/// Initialize the core configuration.
///
/// # Parameters
///
/// * `blob_vaddr` - The DTB blob virtual address.
fn init_core_config(blob_vaddr: usize) {
  unsafe {
    assert!(dtb_cpu::get_core_config(ptr::addr_of_mut!(CORE_CONFIG).as_mut().unwrap(), blob_vaddr));
  }
}

/// Initialize the memory layout configuration.
///
/// # Parameters
///
/// * `blob_vaddr` - The DTB blob virtual address.
/// * `blob_size` - The size of the DTB blob.
///
/// # Description
///
/// Reads the ranges covered by memory devices from the DTB, then excludes any
/// physical memory beyond the virtual base address, excludes 0 to the end of
/// the section-aligned kernel, and excludes the section-aligned DTB area. The
/// remaining physical memory is available for use.
///
/// # Assumptions
///
/// Assumes the system is configured correctly and that there will not be any
/// overflow when calculating end of the kernel or blob.
fn init_memory_config(blob_vaddr: usize, blob_size: usize) {
  let tagger = RangeZoneTagger::new(get_high_mem_base());
  let mem_config = unsafe { ptr::addr_of_mut!(MEMORY_CONFIG).as_mut().unwrap() };
  assert!(dtb_memory::get_memory_layout(mem_config, &tagger, blob_vaddr));

  let kconfig = get_kernel_config();
  let section_size = get_section_size();
  let blob_start = bits::align_down(kconfig.blob, section_size);
  let blob_size = bits::align_up(kconfig.blob + blob_size, section_size) - blob_start;

  let excl = &[
    range::Range::<MemoryZone> {
      tag: MemoryZone::InvalidZone,
      base: kconfig.virtual_base,
      size: usize::MAX - kconfig.virtual_base + 1,
    },
    range::Range::<MemoryZone> {
      tag: MemoryZone::InvalidZone,
      base: 0,
      size: bits::align_up(kconfig.kernel_base + kconfig.kernel_size, section_size),
    },
    range::Range::<MemoryZone> {
      tag: MemoryZone::InvalidZone,
      base: blob_start,
      size: blob_size,
    },
  ];

  for range in excl {
    mem_config.exclude_range(range);
  }
}

/// Initialize the linear memory map.
///
/// # Description
///
/// Linearly maps the low memory area into the kernel page tables. Invalidating
/// the TLB is not required here. We are only adding new entries at this point.
fn init_direct_map() {
  // Calculate the base of the thread local area.
  let core_count = get_core_config().get_core_count();
  let section_size = get_section_size();
  let thread_local_size = section_size * core_count;

  unsafe {
    THREAD_LOCAL_VIRTUAL_BASE =
      bits::align_down(PAGE_DIRECTORY_VIRTUAL_BASE - thread_local_size, section_size);
  }

  // The memory layout already excludes any physical memory beyond the kernel /
  // user split. However, we still need to mask off physical memory that cannot
  // be linearly mapped into the low memory area.
  let mut low_mem = *get_memory_config();
  let high_mem_base = get_high_mem_base();
  let excl = range::Range::<MemoryZone> {
    tag: MemoryZone::InvalidZone,
    base: high_mem_base,
    size: usize::MAX - high_mem_base + 1,
  };

  low_mem.exclude_range(&excl);

  // Construct a linear allocator using the reserved kernel pages area. There
  // will be no more than three bootstrap tables, so start three pages in.
  let kconfig = get_kernel_config();
  let offset = 3 * kconfig.page_size;
  let mut allocator = LinearTableAllocator::new(
    kconfig.kernel_pages_start + offset,
    kconfig.kernel_pages_start + kconfig.kernel_pages_size,
    get_page_size(),
  );

  for range in low_mem.get_ranges() {
    mm::direct_map_memory(
      kconfig.virtual_base,
      kconfig.kernel_pages_start,
      range.base,
      range.size,
      false,
      &mut allocator,
      MappingStrategy::Compact,
    );
  }
}
