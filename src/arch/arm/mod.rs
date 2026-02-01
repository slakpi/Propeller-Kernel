//! ARM Architecture

mod exceptions;
mod mm;

pub mod task;

pub use super::arm_common::{cpu, sync};
pub use super::common::{device_tree, memory};

use super::arm_common::{dtb_cpu, dtb_memory};
use crate::support::{bits, dtb, range};
#[cfg(feature = "module_tests")]
use crate::test;
use core::ptr;
use memory::{
  BufferedPageAllocator, FlexAllocator, MappingStrategy, MemoryConfig, MemoryRange,
  MemoryRangeHandler, MemoryZone,
};

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
const PAGE_DATABASE_SIZE: usize = 24 * 1024 * 1024;

/// The base virtual address of the page directory.
const PAGE_DATABASE_VIRTUAL_BASE: usize = RECURSIVE_MAP_AREA - PAGE_DATABASE_SIZE;

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
};

/// System device tree.
static mut DEVICE_TREE: device_tree::DeviceTree = device_tree::DeviceTree::new();

/// The base virtual address and size of the thread local mapping area.
static mut THREAD_LOCAL_AREA_VIRTUAL_BASE: usize = 0;

static mut THREAD_LOCAL_AREA_SIZE: usize = 0;

/// The base virtual address and size of the ISR stack area.
static mut ISR_STACK_AREA_VIRTUAL_BASE: usize = 0;

static mut ISR_STACK_AREA_SIZE: usize = 0;

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
        tag: MemoryZone::LinearMemoryZone,
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

/// Initialize symmetric multiprocessing.
///
/// # Parameters
///
/// * `allocator` - An allocator suitable for allocating stacks and page tables.
pub fn init_smp(allocator: &mut impl FlexAllocator) {
  if get_device_tree().get_core_config().get_core_count() < 2 {
    return;
  }

  init_isr_stacks(allocator);
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

/// Get the maximum physical address.
///
/// # Description
///
/// The maximum physical address is just the bitwise negation of the kernel's
/// base physical address. For example, with a 3/1 split, the kernel starts at
/// 0xc000_0000 and the maximum physical address is 0x3fff_ffff.
pub fn get_maximum_physical_address() -> usize {
  !get_kernel_base()
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

/// Get the system device tree.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_device_tree() -> &'static device_tree::DeviceTree {
  unsafe { ptr::addr_of!(DEVICE_TREE).as_ref().unwrap() }
}

/// Get the core index of the current core.
///
/// # Description
///
/// For any non-trivial use of the core index, interrupts must be disabled prior
/// to calling to prevent the task from moving to another core.
pub fn get_current_core_index() -> usize {
  get_device_tree()
    .get_core_config()
    .get_core_index(cpu::get_id())
    .unwrap()
}

/// Get the page database virtual base address.
pub fn get_page_database_virtual_base() -> usize {
  unsafe { PAGE_DATABASE_VIRTUAL_BASE }
}

/// Get the size of the page database.
pub fn get_page_database_size() -> usize {
  PAGE_DATABASE_SIZE
}

/// Get the base physical address of the high memory area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
fn get_high_mem_base() -> usize {
  usize::MAX - get_kernel_virtual_base() - HIGH_MEM_SIZE + 1
}

/// Get the base virtual address of the thread local mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_thread_local_area_virtual_base() -> usize {
  unsafe { THREAD_LOCAL_AREA_VIRTUAL_BASE }
}

/// Get the size of the thread local mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_thread_local_area_size() -> usize {
  unsafe { THREAD_LOCAL_AREA_SIZE }
}

/// Get the base virtual address of the ISR stack area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_isr_stack_area_virtual_base() -> usize {
  unsafe { ISR_STACK_AREA_VIRTUAL_BASE }
}

/// Get the size of the ISR stack area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_isr_stack_size() -> usize {
  unsafe { ISR_STACK_AREA_SIZE }
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
  let core_config = unsafe {
    ptr::addr_of_mut!(DEVICE_TREE)
      .as_mut()
      .unwrap()
      .get_core_config_mut()
  };

  assert!(dtb_cpu::get_core_config(core_config, blob_vaddr));
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
  let device_tree = unsafe { ptr::addr_of_mut!(DEVICE_TREE).as_mut().unwrap() };

  let kconfig = get_kernel_config();
  let core_count = device_tree.get_core_config().get_core_count();
  let page_shift = get_page_shift();
  let section_size = get_section_size();
  let blob_start = bits::align_down(kconfig.blob, section_size);
  let blob_size = bits::align_up(kconfig.blob + blob_size, section_size) - blob_start;

  unsafe {
    ISR_STACK_AREA_SIZE = ((kconfig.kernel_stack_pages + 1) << page_shift) * 4 * core_count;
    ISR_STACK_AREA_VIRTUAL_BASE = PAGE_DATABASE_VIRTUAL_BASE - ISR_STACK_AREA_SIZE;
    THREAD_LOCAL_AREA_SIZE = section_size * core_count;
    THREAD_LOCAL_AREA_VIRTUAL_BASE =
      bits::align_down(ISR_STACK_AREA_VIRTUAL_BASE - THREAD_LOCAL_AREA_SIZE, section_size);
  }

  let tagger = RangeZoneTagger::new(get_high_mem_base());
  let mem_config = device_tree.get_memory_config_mut();
  assert!(dtb_memory::get_memory_layout(mem_config, &tagger, blob_vaddr));

  let excl = &[
    // Exclude the page database.
    MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: PAGE_DATABASE_VIRTUAL_BASE - kconfig.virtual_base,
      size: PAGE_DATABASE_SIZE,
    },
    // Exclude the kernel area.
    MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: kconfig.virtual_base,
      size: usize::MAX - kconfig.virtual_base + 1,
    },
    // Exclude from 0 up to the end of the kernel.
    MemoryRange {
      tag: MemoryZone::InvalidZone,
      base: 0,
      size: bits::align_up(kconfig.kernel_base + kconfig.kernel_size, section_size),
    },
    // Exclude the DTB blob.
    MemoryRange {
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
/// Linearly maps physical memory into the kernel page tables. Invalidating the
/// TLB is not required here. We are only adding new entries at this point.
fn init_direct_map() {
  let kconfig = get_kernel_config();

  // The memory layout already excludes any physical memory beyond the kernel /
  // user split. However, we still need to mask off physical memory that cannot
  // be linearly mapped.
  let high_mem_base = get_high_mem_base();
  let excl = MemoryRange {
    tag: MemoryZone::InvalidZone,
    base: high_mem_base,
    size: usize::MAX - high_mem_base + 1,
  };

  // After initial setup, there should be a maximum of four page tables in use.
  // That leaves 12, so a single 32-bit word is enough for the allocator's
  // bitmap.
  let offset = 4 * kconfig.page_size;
  let mut allocator = BufferedPageAllocator::<1>::new(
    kconfig.kernel_pages_start + offset,
    kconfig.kernel_pages_start + kconfig.kernel_pages_size,
    get_page_size(),
  );

  // Linearly map each memory range using 2 MiB sections. For each range in the
  // memory configuration, exclude the high memory area. This adds roughly the
  // same amount of time overhead as copying the memory configuration and
  // excluding the high memory area from the set but does not incur the stack
  // space or time cost of copying the configuration.
  for range in get_device_tree().get_memory_config().get_ranges() {
    let (left, right) = range.exclude(&excl).unwrap();

    if let Some(left) = left {
      mm::direct_map_memory(
        kconfig.virtual_base,
        kconfig.kernel_pages_start,
        left.base,
        left.size,
        false,
        &mut allocator,
        MappingStrategy::Compact,
      );
    }

    if let Some(right) = right {
      mm::direct_map_memory(
        kconfig.virtual_base,
        kconfig.kernel_pages_start,
        right.base,
        right.size,
        false,
        &mut allocator,
        MappingStrategy::Compact,
      );
    }
  }
}

/// Initialize the ISR stacks.
///
/// # Parameters
///
/// * `allocator` - Page table and stack allocator.
///
/// # Description
///
/// Allocates SVC, ABT, IRQ, and FIQ for each secondary core, maps the stacks
/// into the ISR stack area, and adds entries to the stack list. The primary
/// core's stacks will have already been mapped to the end of the ISR stack
/// area. Cores 1..N will be placed in the ISR stack area starting at the
/// beginning.
///
/// # Assumptions
///
/// Assumes multiple cores.
fn init_isr_stacks(allocator: &mut impl FlexAllocator) {
  let kconfig = get_kernel_config();
  let core_config = get_device_tree().get_core_config();
  let page_shift = get_page_shift();
  let stack_size = kconfig.kernel_stack_pages << page_shift;
  let step_size = (kconfig.kernel_stack_pages + 1) << page_shift;
  let stack_area_base = get_isr_stack_area_virtual_base();

  for (index, core) in core_config.get_cores().iter().enumerate() {
    // Skip the primary core.
    if index == 0 {
      continue;
    }

    // Each stack list entry is the core ID + 4 stack addresses.
    let entry_offset = (index * 5) << bits::WORD_SHIFT;
    let ptr = (kconfig.virtual_base + kconfig.kernel_stack_list + entry_offset) as *mut usize;

    unsafe {
      *ptr = core.get_id();
    }

    // Calculate the virtual base address for the stacks.
    let mut stack_vbase = stack_area_base + (step_size * 4 * (index - 1)) + (1 << page_shift);

    for s in 1..=4 {
      // We must successfully allocate a stack for each core.
      let (stack_base, _) = allocator
        .contiguous_alloc(kconfig.kernel_stack_pages)
        .unwrap();

      unsafe {
        *ptr.add(s) = stack_vbase + stack_size;
      }

      // Map the core's stack into the ISR stack area.
      mm::map_memory(
        kconfig.virtual_base,
        kconfig.kernel_pages_start,
        stack_vbase,
        stack_base,
        stack_size,
        false,
        allocator,
        MappingStrategy::Granular,
      );

      stack_vbase += step_size;
    }
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut test::TestContext) {
  task::run_tests(context);
}
