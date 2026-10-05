use super::mm;
use crate::arch::arm_common::{dtb_cpu, dtb_memory};
use crate::arch::{cpu, debug};
use crate::debug_print;
use crate::mm::buffered_allocator::BufferedPageAllocator;
use crate::support::memory::{
  MappingStrategy, MemoryConfig, MemoryRange, MemoryRangeHandler, MemoryZone, PageAllocator,
};
use crate::support::{bits, device_tree, dtb};
use crate::sync::SpinLock;
use core::{ptr, slice};

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
const RECURSIVE_MAP_AREA_VIRTUAL_BASE: usize = 0xffc0_0000;

/// The size of the virtual area reserved for the page directory.
const PAGE_DATABASE_SIZE: usize = 24 * 1024 * 1024;

/// The base virtual address of the page directory.
const PAGE_DATABASE_VIRTUAL_BASE: usize = RECURSIVE_MAP_AREA_VIRTUAL_BASE - PAGE_DATABASE_SIZE;

/// Each core has five different ISR stacks.
const ISR_STACKS_PER_CORE: usize = 5;

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

/// The base virtual address of the driver area.
///
///   NOTE: If using serial debug output, the driver virtual base will be
///         incremented to reserve the mappings for the UART. This value must
///         not be modified once initialization is complete.
static mut DRIVER_VIRTUAL_BASE: usize = 0xf800_0000;

/// The base virtual address and size of the thread local mapping area.
///
///   NOTE: These must not be modified once initialization is complete.
static mut THREAD_LOCAL_AREA_VIRTUAL_BASE: usize = 0;
static mut THREAD_LOCAL_AREA_SIZE: usize = 0;

/// The base virtual address and size of the ISR stack area.
///
///   NOTE: These must not be modified once initialization is complete.
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

  let kconfig = unsafe { (config_addr as *const KernelConfig).as_ref().unwrap() };

  unsafe {
    KERNEL_CONFIG = *kconfig;
  }

  // After initial setup, there should be a maximum of four page tables in use.
  // That leaves 12, so a single 32-bit word is enough for the allocator's
  // bitmap.
  let offset = 4 * kconfig.page_size;
  let mut allocator = BufferedPageAllocator::<1>::new(
    kconfig.kernel_pages_start + offset,
    kconfig.kernel_pages_start + kconfig.kernel_pages_size,
    get_page_size(),
  );

  #[cfg(feature = "serial_debug_output")]
  init_serial_debug_output(
    kconfig.virtual_base,
    kconfig.vm_split,
    kconfig.kernel_pages_start,
    &mut allocator,
  );

  debug_print!("\n\n=== Propeller (ARM 32-bit) ===\n");
  debug_print!("Booting on core {:x}...\n", cpu::get_id());
  debug_print!("arch init...\n");

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

  init_core_config(blob_vaddr);
  init_memory_config(blob_vaddr, blob_size);
  init_direct_map(&mut allocator);
}

/// Initialize symmetric multiprocessing.
///
/// # Parameters
///
/// * `allocator` - An allocator suitable for allocating stacks and page tables.
pub fn init_smp(allocator: &SpinLock<impl PageAllocator>) {
  let core_config = get_device_tree().get_core_config();

  // If the system only has a single core, it is already set up. There is no
  // need to do anything else.
  if core_config.get_core_count() < 2 {
    return;
  }

  debug_print!("--- SMP Initialization ---\n");
  init_isr_stacks(allocator.lock().as_mut());

  debug_print!("Releasing secondary cores...\n");
  let start_addr = _secondary_start as *const () as usize - get_kernel_virtual_base();
  core_config.release(start_addr);
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
  PAGE_DATABASE_VIRTUAL_BASE
}

/// Get the size of the page database.
pub fn get_page_database_size() -> usize {
  PAGE_DATABASE_SIZE
}

/// Get the physical base address of the kernel page table area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_kernel_pages_start() -> usize {
  unsafe { KERNEL_CONFIG.kernel_pages_start }
}

/// Get the size of the kernel page table area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_kernel_pages_size() -> usize {
  unsafe { KERNEL_CONFIG.kernel_pages_start }
}

/// Get the virtual memory split.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
pub fn get_vm_split() -> usize {
  unsafe { KERNEL_CONFIG.vm_split }
}

/// Get the base physical address of the high memory area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
pub fn get_high_mem_base() -> usize {
  usize::MAX - get_kernel_virtual_base() - HIGH_MEM_SIZE + 1
}
/// Get the virtual base of the driver I/O mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
pub fn get_driver_area_virtual_base() -> usize {
  unsafe { DRIVER_VIRTUAL_BASE }
}

/// Get the size of the driver I/O mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
pub fn get_driver_area_size() -> usize {
  unsafe { THREAD_LOCAL_AREA_VIRTUAL_BASE - DRIVER_VIRTUAL_BASE }
}

/// Get the virtual base of the recursive mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
pub fn get_recursive_map_area_virtual_base() -> usize {
  unsafe { RECURSIVE_MAP_AREA_VIRTUAL_BASE }
}

/// Get the base virtual address of the thread local mapping area.
///
/// # Description
///
///   NOTE: Private to the ARM architecture
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_thread_local_area_virtual_base() -> usize {
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
pub fn get_thread_local_area_size() -> usize {
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
pub fn get_isr_stack_area_virtual_base() -> usize {
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
pub fn get_isr_stack_size() -> usize {
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

/// Initialize low-level serial debug output.
///
/// # Parameters
///
/// * `virt_base` - The virtual base address.
/// * `pages_start` - The physical address of the first kernel page table.
#[cfg(feature = "serial_debug_output")]
fn init_serial_debug_output(
  virt_base: usize,
  vm_split: usize,
  pages_start: usize,
  allocator: &mut impl PageAllocator,
) {
  let range = debug::get_physical_range();

  assert!(bits::is_aligned(range.0, get_page_size()));

  let size = range.1 << get_page_shift();
  if size == 0 {
    return;
  }

  let virt_addr = unsafe { DRIVER_VIRTUAL_BASE };

  mm::map_memory(
    virt_base,
    vm_split,
    pages_start,
    virt_addr,
    range.0,
    size,
    true,
    allocator,
    MappingStrategy::Granular,
  );

  debug::init(virt_addr);

  unsafe { DRIVER_VIRTUAL_BASE += size };
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

  debug_print!(" Cores:\n");
  for core in core_config.get_cores() {
    let s = core::str::from_utf8(&core.core_type).unwrap_or("Unknown");
    debug_print!("  {:x}: {}\n", core.id, s)
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
  let device_tree = unsafe { ptr::addr_of_mut!(DEVICE_TREE).as_mut().unwrap() };

  let kconfig = get_kernel_config();
  let core_count = device_tree.get_core_config().get_core_count();
  let page_shift = get_page_shift();
  let section_size = get_section_size();
  let blob_start = bits::align_down(kconfig.blob, section_size);
  let blob_size = bits::align_up(kconfig.blob + blob_size, section_size) - blob_start;

  unsafe {
    ISR_STACK_AREA_SIZE =
      ((kconfig.kernel_stack_pages + 1) << page_shift) * ISR_STACKS_PER_CORE * core_count;
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

  debug_print!(" Memory: \n");
  for range in mem_config.get_ranges() {
    debug_print!("  {:#x} - {:#x}\n", range.base, range.base + range.size - 1);
  }
}

/// Initialize the linear memory map.
///
/// # Description
///
/// Linearly maps physical memory into the kernel page tables. Invalidating the
/// TLB is not required here. We are only adding new entries at this point.
fn init_direct_map(allocator: &mut impl PageAllocator) {
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

  // Linearly map each memory range using 2 MiB sections. For each range in the
  // memory configuration, exclude the high memory area. This adds roughly the
  // same amount of time overhead as copying the memory configuration and
  // excluding the high memory area from the set but does not incur the stack
  // space or time cost of copying the configuration.
  debug_print!(" Linear Kernel Memory Mappings:\n");
  for range in get_device_tree().get_memory_config().get_ranges() {
    let (left, _) = range.exclude(&excl).unwrap();

    if let Some(left) = left {
      mm::direct_map_memory(
        kconfig.virtual_base,
        kconfig.vm_split,
        kconfig.kernel_pages_start,
        left.base,
        left.size,
        false,
        allocator,
        MappingStrategy::Compact,
      );

      debug_print!(
        "  {:#x} - {:#x} => {:#x}\n",
        left.base,
        left.base + left.size - 1,
        kconfig.virtual_base + left.base
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
/// Allocates SVC, IRQ, ABT, UND, and FIQ for each secondary core, maps the
/// stacks into the ISR stack area, and adds entries to the stack list. The
/// primary core's stacks will have already been mapped to the end of the ISR
/// stack area. Cores 1..N will be placed in the ISR stack area starting at the
/// beginning.
///
/// # Assumptions
///
/// Assumes multiple cores.
fn init_isr_stacks(allocator: &mut impl PageAllocator) {
  // Each entry has the core index and the five stack addresses.
  const ENTRY_LEN: usize = ISR_STACKS_PER_CORE + 1;

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
  let mut entry_index = ENTRY_LEN;

  debug_print!("ISR Stacks:\n");
  debug_print!(
    " Core {:x}: FIQ {:#x}, UND {:#x}, ABT {:#x}, IRQ {:#x}, SVC {:#x}\n",
    table[0],
    table[1],
    table[2],
    table[3],
    table[4],
    table[5],
  );

  for (index, core) in core_config.get_cores().iter().enumerate().skip(1) {
    table[entry_index] = core.id;

    // Calculate the virtual base address for the stacks.
    let mut stack_vbase =
      stack_area_base + (step_size * ISR_STACKS_PER_CORE * (index - 1)) + (1 << page_shift);

    for s in 1..ENTRY_LEN {
      // We must successfully allocate stacks for each core.
      let (stack_base, _) = allocator.alloc(kconfig.kernel_stack_pages).unwrap();

      table[entry_index + s] = stack_vbase + stack_size;

      // Map the core's stack into the ISR stack area.
      mm::map_memory(
        kconfig.virtual_base,
        kconfig.vm_split,
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

    debug_print!(
      " Core {:x}: FIQ {:#x}, UND {:#x}, ABT {:#x}, IRQ {:#x}, SVC {:#x}\n",
      table[entry_index],
      table[entry_index + 1],
      table[entry_index + 2],
      table[entry_index + 3],
      table[entry_index + 4],
      table[entry_index + 5],
    );

    entry_index += ENTRY_LEN;
  }
}
