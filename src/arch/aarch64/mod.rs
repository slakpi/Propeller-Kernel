//! AArch64 Architecture

mod exceptions;
mod mm;

pub mod task;

#[cfg(feature = "serial_debug_output")]
pub use super::arm_common::debug;
pub use super::arm_common::{cpu, sync};
pub use super::common::{device_tree, memory};

use super::arm_common::{dtb_cpu, dtb_memory};
use crate::arch::memory::PageAllocator;
use crate::debug_print;
use crate::support::{bits, dtb, range};
#[cfg(feature = "module_tests")]
use crate::test;
use core::{ptr, slice};
use memory::{
  BufferedPageAllocator, FlexAllocator, MappingStrategy, MemoryConfig, MemoryRange,
  MemoryRangeHandler, MemoryZone,
};

unsafe extern "C" {
  fn _secondary_start();
}

/// Propeller requires 4 KiB pages and uses 2 MiB seconds at Level 3. All page
/// table entries are 8 bytes.
const PAGE_SIZE: usize = 4096;

const PAGE_SHIFT: usize = 12;

const PAGE_MASK: usize = PAGE_SIZE - 1;

const SECTION_SIZE: usize = 2 * 1024 * 1024;

const SECTION_SHIFT: usize = 21;

const SECTION_MASK: usize = SECTION_SIZE - 1;

const PAGE_TABLE_ENTRY_SIZE: usize = 8;

const PAGE_TABLE_ENTRY_SHIFT: usize = 3;

/// The size of the virtual area reserved for the page directory (2 TiB).
const PAGE_DATABASE_SIZE: usize = 0x200_0000_0000;

/// The base virtual address of the page directory.
const PAGE_DATABASE_VIRTUAL_BASE: usize = 0xffff_fe00_0000_0000;

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
  kernel_stack_list: 0,
  kernel_stack_pages: 0,
};

/// System device tree.
static mut DEVICE_TREE: device_tree::DeviceTree = device_tree::DeviceTree::new();

/// The base virtual address and size of the ISR stack area.
static mut ISR_STACK_AREA_VIRTUAL_BASE: usize = 0;

static mut ISR_STACK_AREA_SIZE: usize = 0;

/// Tags memory ranges with the appropriate zone.
pub struct RangeZoneTagger {}

impl MemoryRangeHandler for RangeZoneTagger {
  /// See `MemoryRangeHandler::handle_range()`.
  ///
  /// # Description
  ///
  /// All memory in an 64-bit platform is linear memory.
  fn handle_range(&self, config: &mut MemoryConfig, base: usize, size: usize) {
    config.insert_range(MemoryRange {
      tag: MemoryZone::LinearMemoryZone,
      base,
      size,
    });
  }
}

/// AArch64 platform configuration.
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

  unsafe {
    KERNEL_CONFIG = *kconfig;
  }

  // Construct a buffered, linear allocator using the reserved kernel pages
  // area. There will be no more than six bootstrap tables, so start six pages
  // in.
  let offset = 6 * get_page_size();

  // Six pages are used leaving 10 available, so the allocator only needs one
  // 64-bit word for its bitmap.
  let mut allocator = BufferedPageAllocator::<1>::new(
    kconfig.kernel_pages_start + offset,
    kconfig.kernel_pages_start + kconfig.kernel_pages_size,
    get_page_size(),
  );

  #[cfg(feature = "serial_debug_output")]
  init_serial_debug_output(kconfig.virtual_base, kconfig.kernel_pages_start, &mut allocator);

  debug_print!("=== Propeller (AArch64) ===\n");
  debug_print!("Booting on core {:x}.\n", cpu::get_id());

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

  init_core_config(blob_vaddr);
  init_memory_config(blob_vaddr, blob_size);
  init_direct_map(&mut allocator);

  debug_print!("arch init complete.\n");
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

  debug_print!("arch SMP init complete.\n");
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
/// base physical address. For example, if the kernel starts at
/// 0xffff_0000_0000_0000, the maximum physical address is
/// 0x0000_ffff_ffff_ffff.
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
  PAGE_DATABASE_VIRTUAL_BASE
}

/// Get the page database size.
pub fn get_page_database_size() -> usize {
  PAGE_DATABASE_SIZE
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
///   NOTE: Private to the AArch64 architecture.
fn get_kernel_config() -> &'static KernelConfig {
  unsafe { ptr::addr_of!(KERNEL_CONFIG).as_ref().unwrap() }
}

/// Initialize low-level serial debug output.
///
/// # Parameters
///
/// * `virt_base` - The virtual base address.
/// * `pages_start` - The physical address of the first kernel page table.
#[cfg(feature = "serial_debug_output")]
fn init_serial_debug_output(
  virt_base: usize,
  pages_start: usize,
  allocator: &mut impl PageAllocator,
) {
  let range = debug::get_physical_range();

  mm::map_memory(
    virt_base,
    pages_start,
    virt_base + range.0,
    range.0,
    range.1,
    true,
    allocator,
    MappingStrategy::Granular,
  );

  debug::init(virt_base + range.0);
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

  for core in core_config.get_cores() {
    let s = core::str::from_utf8(&core.get_core_type()).unwrap_or("Unknown");
    debug_print!("Core {:x}: {}\n", core.get_id(), s)
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
/// overflow when calculating end of the kernel or blob..
fn init_memory_config(blob_vaddr: usize, blob_size: usize) {
  let device_tree = unsafe { ptr::addr_of_mut!(DEVICE_TREE).as_mut().unwrap() };

  let kconfig = get_kernel_config();
  let core_count = device_tree.get_core_config().get_core_count();
  let page_shift = get_page_shift();
  let section_size = get_section_size();
  let blob_start = bits::align_down(kconfig.blob, section_size);
  let blob_size = bits::align_up(kconfig.blob + blob_size, section_size) - blob_start;

  unsafe {
    ISR_STACK_AREA_SIZE = ((kconfig.kernel_stack_pages + 1) << page_shift) * core_count;
    ISR_STACK_AREA_VIRTUAL_BASE = PAGE_DATABASE_VIRTUAL_BASE - ISR_STACK_AREA_SIZE;
  }

  let tagger = RangeZoneTagger {};
  let mem_config = device_tree.get_memory_config_mut();
  assert!(dtb_memory::get_memory_layout(mem_config, &tagger, blob_vaddr));

  let excl = &[
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

  for range in mem_config.get_ranges() {
    debug_print!("Memory: {:#x} - {:#x}\n", range.base, range.base + range.size - 1);
  }
}

/// Initialize the linear memory map.
///
/// # Parameters
///
/// * `allocator` - The allocator that will provide new table pages.
///
/// # Description
///
/// Linearly maps physical memory into the kernel page tables. Invalidating the
/// TLB is not required here. We are only adding new entries at this point.
fn init_direct_map(allocator: &mut impl PageAllocator) {
  let kconfig = get_kernel_config();
  let mem_config = get_device_tree().get_memory_config();

  // Linearly map each memory range using 2 MiB sections.
  for range in mem_config.get_ranges() {
    mm::direct_map_memory(
      kconfig.virtual_base,
      kconfig.kernel_pages_start,
      range.base,
      range.size,
      false,
      allocator,
      MappingStrategy::Compact,
    );

    debug_print!(
      "Map: {:#x} - {:#x} => {:#x}\n",
      range.base,
      range.base + range.size - 1,
      kconfig.virtual_base + range.base
    );
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
/// Allocates an ISR stack for each secondary core, maps the stacks into the ISR
/// stack area, and adds entries to the stack list. The primary core's stack
/// will have already been mapped to the end of the ISR stack area. Cores 1..N
/// will be placed in the ISR stack area starting at the beginning.
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
  let table = unsafe {
    slice::from_raw_parts_mut(
      (kconfig.virtual_base + kconfig.kernel_stack_list) as *mut usize,
      kconfig.page_size,
    )
  };
  let mut entry_index = 2;

  debug_print!(
    "Core {:x}: EL1 {:#x}\n",
    table[0],
    table[1],
  );

  for (index, core) in core_config.get_cores().iter().enumerate().skip(1) {
    // We must successfully allocate a stack for each core.
    let (stack_base, _) = allocator
      .contiguous_alloc(kconfig.kernel_stack_pages)
      .unwrap();

    // Each stack list entry is the core ID + stack address.
    let entry_offset = (index * 2) << bits::WORD_SHIFT;
    let ptr = (kconfig.virtual_base + kconfig.kernel_stack_list + entry_offset) as *mut usize;

    // Calculate the virtual base address for the stack and update the stack
    // list with the core ID and stack start address.
    let stack_vbase = stack_area_base + (step_size * (index - 1)) + (1 << page_shift);
    table[entry_index] = core.get_id();
    table[entry_index + 1] = stack_vbase + stack_size;

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

    debug_print!(
      "Core {:x}: EL1 {:#x}\n",
      table[entry_index],
      table[entry_index + 1],
    );

    entry_index += 2;
  }
}

#[cfg(feature = "module_tests")]
pub fn run_tests() {
  let mut context = test::TestContext::new();
  crate::arch::task::run_tests(&mut context);
  debug_print!(" arch: {} pass, {} fail\n", context.pass_count, context.fail_count);
}
