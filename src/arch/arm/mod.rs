//! ARM Architecture

mod exceptions;
mod mm;

pub mod task;

use crate::arch::{cpu, memory};
use crate::mm::{MappingStrategy, table_allocator::LinearTableAllocator};
use crate::support::{bits, dtb, range};
use crate::task::Task;
use core::{ptr, slice};
use core::arch::asm;

/// Propeller requires LPAE. With LPAE enabled, pages must be 4 KiB and sections
/// are 2 MiB at Level 2.
const PAGE_SIZE: usize = 4096;

const PAGE_SHIFT: usize = 12;

const PAGE_MASK: usize = PAGE_SIZE - 1;

const SECTION_SIZE: usize = 2 * 1024 * 1024;

const SECTION_SHIFT: usize = 21;

const SECTION_MASK: usize = SECTION_SIZE - 1;

/// Reserve the upper 128 MiB of the kernel segment for the high memory area.
const HIGH_MEM_SIZE: usize = 128 * 1024 * 1024;

/// The base virtual address of the exception vectors.
const VECTORS_VIRTUAL_BASE: usize = 0xffff_0000;

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
static mut CORE_CONFIG: cpu::CoreConfig = [cpu::Core::new(); cpu::MAX_CORES];

/// Memory layout configuration.
static mut MEMORY_CONFIG: memory::MemoryConfig = memory::MemoryConfig::new();

/// The base virtual address of the thread local mapping area.
static mut THREAD_LOCAL_VIRTUAL_BASE: usize = 0;

/// The bootstrap task is a special task that exists only to provide a way to
/// manage high memory mappings before the kernel allocators and scheduler are
/// initialized. The bootstrap task will only be used by the primary core.
///
/// Once the kernel maps system memory, initializes the kernel allocators,
/// initializes the scheduler, and enables the secondary cores, the bootstrap
/// task will be replaced by the real init thread tasks.
static mut BOOTSTRAP_TASK: Task = Task::new(0);

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
  init_bootstrap_task();

  let lcl_addr = Task::map_page_local(0x3900_0000, false);

  let mut test_addr = 0usize;
  unsafe {
    asm!(
      "mcr p15, 0, {in_addr}, c7, c8, 0",
      "isb",
      "mrc p15, 0, {out_addr}, c7, c4, 0",
      in_addr = in(reg) lcl_addr,
      out_addr = inout(reg) test_addr,
    );
  }

  let lcl_page = unsafe { slice::from_raw_parts_mut(lcl_addr as *mut usize, 1024) };
  lcl_page[0] = 42;

  let lcl_addr2 = Task::map_page_local(0x3900_0000, false);
  let lcl_page2 = unsafe { slice::from_raw_parts_mut(lcl_addr2 as *mut usize, 1024) };

  Task::unmap_page_local();
  Task::unmap_page_local();
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

/// Get the base virtual address of the thread local area for the current core.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
fn get_thread_local_virtual_base() -> usize {
  let offset = cpu::get_id() * get_section_size();
  unsafe { THREAD_LOCAL_VIRTUAL_BASE + offset }
}

/// Get the base physical address of the high memory area.
fn get_high_mem_base() -> usize {
  usize::MAX - get_kernel_virtual_base() - HIGH_MEM_SIZE + 1
}

/// Get the number of cores.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_core_count() -> usize {
  unsafe { get_core_config().len() }
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

/// Get the memory layout configuration.
///
/// # Description
///
///   NOTE: The interface guarantees read-only access outside of the module and
///         one-time initialization is assumed.
pub fn get_memory_config() -> &'static memory::MemoryConfig {
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
    assert!(cpu::get_core_config(ptr::addr_of_mut!(CORE_CONFIG).as_mut().unwrap(), blob_vaddr));
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
  let mem_config = unsafe { ptr::addr_of_mut!(MEMORY_CONFIG).as_mut().unwrap() };
  assert!(memory::get_memory_layout(mem_config, blob_vaddr));

  let kconfig = get_kernel_config();
  let section_size = get_section_size();
  let blob_start = bits::align_down(kconfig.blob, section_size);
  let blob_size = bits::align_up(kconfig.blob + blob_size, section_size) - blob_start;

  let excl = &[
    range::Range {
      base: kconfig.virtual_base,
      size: usize::MAX - kconfig.virtual_base + 1,
    },
    range::Range {
      base: 0,
      size: bits::align_up(kconfig.kernel_base + kconfig.kernel_size, section_size),
    },
    range::Range {
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
/// Linearly maps the low memory area into the kernel page tables.
fn init_direct_map() {
  // Calculate the base of the thread local area.
  let core_count = get_core_count();
  let section_size = get_section_size();
  let thread_local_size = section_size * core_count;

  unsafe {
    THREAD_LOCAL_VIRTUAL_BASE = bits::align_down(
      PAGE_DIRECTORY_VIRTUAL_BASE - thread_local_size,
      section_size,
    );
  }

  // The memory layout already excludes any physical memory beyond the kernel /
  // user split. However, we still need to mask off physical memory that cannot
  // be linearly mapped into the low memory area.
  let mut low_mem = *get_memory_config();
  let high_mem_base = get_high_mem_base();
  let excl = range::Range {
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
  
  mm::invalidate_caches();
}

/// Initialize the bootstrap task.
///
/// # Description
///
/// There is no need to go through the normal context switch process. The
/// bootstrap task is technically already running. We only need to set the
/// running task pointer on the primary core and map the task's local mapping
/// table into the kernel page tables.
///
/// # Assumptions
///
/// Assumes the caller is running on the primary core.
pub fn init_bootstrap_task() {
  let task_ptr = unsafe { ptr::addr_of!(BOOTSTRAP_TASK) };
  let task = unsafe { task_ptr.as_ref().unwrap() };
  let table_vaddr = task.get_context().get_local_mapping_table_addr();
  let table_addr = table_vaddr - get_kernel_virtual_base();
  let pages_start = get_kernel_config().kernel_pages_start;

  task::set_current_task_addr(task_ptr as usize);

  // Map the task's local mapping table into the kernel address space. The
  // assumption is that the caller is running on the primary core, so the table
  // maps to the beginning of the thread-local area.
  mm::map_thread_local_table(pages_start, get_thread_local_virtual_base(), table_addr);
}
