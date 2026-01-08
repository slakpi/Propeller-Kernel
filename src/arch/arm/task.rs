//! ARM Task Management

use super::mm;
use crate::arch::cpu;
use crate::support::bits;
use core::{ptr, slice};

unsafe extern "C" {
  fn task_get_current_task_addr() -> usize;
  fn task_set_current_task_addr(task: usize);
}

const CPU_MASK_WORDS: usize = (cpu::MAX_CORES + usize::BITS as usize - 1) / usize::BITS as usize;

pub type AffinityMask = [usize; CPU_MASK_WORDS];

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// Helper type to force alignment of the local mapping table. The compiler will
/// ensure the local mapping table is aligned to a page boundary and rearrange
/// the remaining fields of the task structure accordingly.
#[repr(C, align(4096))]
struct AlignedTable([usize; 1024]);

/// The bootstrap task's local mapping table.
static mut BOOTSTRAP_LOCAL_TABLE: AlignedTable = AlignedTable([0; 1024]);

/// ARM task context.
///
///   TODO: Add floating-point registers for user tasks.
///
///   TODO: Implement context switching.
pub struct TaskContext {
  r4: usize,
  r5: usize,
  r6: usize,
  r7: usize,
  r8: usize,
  r10: usize,
  fp: usize, // r11, the frame pointer
  sp: usize, // r13, the stack pointer
  pc: usize, // r14, the link register
  table_addr: usize,
  map_count: usize,
  pin_mask: Option<AffinityMask>,
}

impl TaskContext {
  /// Construct an empty task context.
  pub const fn default() -> Self {
    TaskContext {
      r4: 0,
      r5: 0,
      r6: 0,
      r7: 0,
      r8: 0,
      r10: 0,
      fp: 0,
      sp: 0,
      pc: 0,
      table_addr: 0,
      map_count: 0,
      pin_mask: None,
    }
  }

  /// Construct a new task context and allocate a thread-local page table.
  pub fn new() -> Self {
    // TODO: allocate and attach thread-local page table.
    Self::default()
  }

  /// Get the context's local mapping table physical address.
  pub fn get_table_addr(&self) -> usize {
    self.table_addr
  }

  /// Get the current pin mask.
  pub fn get_pin_mask(&self) -> Option<AffinityMask> {
    self.pin_mask
  }

  /// Maps a page into the kernel's virtual address space using the thread-local
  /// mapping table.
  ///
  /// # Parameters
  ///
  /// * `page_addr` - The physical address of the page to map.
  ///
  /// # Description
  ///
  /// See `Task::map_page()`.
  ///
  /// If the page is in low memory, the function adds a null entry to the local
  /// mapping table and returns the virtual address of the linearly mapped page.
  /// This means that pages in low memory still count toward the number of local
  /// mappings a task is maintaining.
  ///
  ///   NOTE: This is done to simplify the unmapping logic which does not take
  ///         an address parameter.
  ///
  /// Otherwise, if the page is in high memory, the function maps the page to
  /// the next available virtual address in the task's local mappings. The
  /// mappings are thread-local, so the function is thread safe.
  ///
  /// The function will panic if no more pages can be mapped into the thread's
  /// local mappings.
  ///
  /// When at least one local mapping exists to a high memory page, the task
  /// will be pinned to the current core. If the task is swapped out while local
  /// mappings to high memory exist, it must be swapped back to the same core
  /// for pointers to remain valid.
  ///
  /// This process is slow, but as pointed out in the announcement of local
  /// mapping in Linux: accessing high memory is slow regardless. If the
  /// processor supports AArch64 and the system has more than 896 MiB of RAM, an
  /// AArch64 build should be used.
  ///
  /// # Returns
  ///
  /// The virtual address of the mapped page.
  pub fn map_page(&mut self, page_addr: usize) -> usize {
    // If mapping a page in low memory, return the linearly mapped address and
    // increment the map count. We do not need to pin the process to the current
    // core.
    if page_addr < super::get_high_mem_base() {
      self.map_count += 1;
      return super::get_kernel_virtual_base() + page_addr;
    }

    // TODO: Interrupts need to be disabled before proceeding to ensure a
    //       context switch does not happen before the task is pinned.

    let core_idx = super::get_current_core_index();

    // Pin the core to the current core if not already pinned. If already
    // pinned, we should be on the same core.
    if self.pin_mask.is_none() {
      let mut pin_mask = AffinityMask::default();
      bits::set_bit(&mut pin_mask, core_idx);
      self.pin_mask = Some(pin_mask);
    }

    // TODO: Interrupts may be re-enabled here; the rest is thread-safe.

    let local_base = super::get_thread_local_virtual_base(core_idx);
    let table_vaddr = super::get_page_virtual_address_for_virtual_address(local_base);
    let table = unsafe { slice::from_raw_parts_mut(table_vaddr.unwrap() as *mut usize, 1024) };
    let page_vaddr = mm::map_page_local(table, local_base, page_addr, self.map_count, false);

    self.map_count += 1;
    page_vaddr
  }

  /// Unmaps the last mapped page in the current task's local mapping table.
  ///
  /// # Description
  ///
  /// If no more local mappings exist after unmapping the last page, the task
  /// will no longer be pinned.
  pub fn unmap_page(&mut self) {
    if self.map_count == 0 {
      return;
    }

    let local_base = super::get_thread_local_virtual_base(super::get_current_core_index());
    let table_vaddr = super::get_page_virtual_address_for_virtual_address(local_base);
    let table = unsafe { slice::from_raw_parts_mut(table_vaddr.unwrap() as *mut usize, 1024) };

    mm::unmap_page_local(table, local_base, self.map_count);

    self.map_count -= 1;

    // No pages mapped, unpin the core.
    if self.map_count == 0 {
      self.pin_mask = None;
    }
  }
}

/// Initialize the bootstrap task context.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
pub fn init_bootstrap_context() -> TaskContext {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  let table_vaddr = unsafe { ptr::addr_of!(BOOTSTRAP_LOCAL_TABLE) as usize };

  // Set up the bootstrap local mapping table.
  //
  //   NOTE: The bootstrap task's local mapping table is part of the kernel
  //         image in low memory. It is safe to just subtract the virtual base
  //         to get the physical address.
  let table_addr = table_vaddr - super::get_kernel_virtual_base();

  // Map the task's local mapping table into the kernel address space using the
  // current core's table slot.
  mm::map_thread_local_table(
    super::get_kernel_config().kernel_pages_start,
    super::get_thread_local_virtual_base(super::get_current_core_index()),
    table_addr,
  );

  let mut context = TaskContext::default();
  context.table_addr = table_addr;
  context
}

/// Get the current task address from the task register.
pub fn get_current_task_addr() -> usize {
  unsafe { task_get_current_task_addr() }
}

/// Set the task register to a new task object address.
///
/// # Parameters
///
/// * `addr` - The new task address.
pub fn set_current_task_addr(addr: usize) {
  unsafe { task_set_current_task_addr(addr) }
}
