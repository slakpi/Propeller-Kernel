//! ARM Task Management

use super::{cpu, mm};
use crate::support::bits;
use core::slice;

unsafe extern "C" {
  fn task_get_current_task_addr() -> usize;
  fn task_set_current_task_addr(task: usize);
}

const CPU_MASK_WORDS: usize = (cpu::MAX_CORES + usize::BITS as usize - 1) / usize::BITS as usize;

pub type AffinityMask = [usize; CPU_MASK_WORDS];

/// ARM task context.
///
///   TODO: Add floating-point registers for user tasks.
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
  /// Construct a new task context.
  ///
  /// # Parameters
  ///
  /// * `table_addr` - The physical address of the local mapping table.
  pub const fn new(table_addr: usize) -> Self {
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
      table_addr,
      map_count: 0,
      pin_mask: None,
    }
  }

  /// Get the context's local mapping table physical address.
  pub fn get_table_addr(&self) -> usize {
    self.table_addr
  }

  /// Set the context's local mapping table address.
  ///
  /// # Parameters
  ///
  /// * `table_addr` - The new local mapping table physical address.
  ///
  /// # Description
  pub fn set_table_addr(&mut self, table_addr: usize) {
    self.table_addr = table_addr;
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
  /// When at least one local mapping exists, the task will be pinned to a core.
  /// If the task is swapped out while local mappings exist, it must be swapped
  /// back to the same core for pointers to locally mapped pages to remain
  /// valid.
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
    let idx = super::get_core_config().get_current_core_index();
    let mut page_vaddr: usize;

    if page_addr < super::get_high_mem_base() {
      page_vaddr = super::get_kernel_virtual_base() + page_addr;
    } else {
      let local_base = super::get_thread_local_virtual_base() + (idx * super::get_section_size());
      let table_vaddr = super::get_page_virtual_address_for_virtual_address(local_base);
      let table = unsafe { slice::from_raw_parts_mut(table_vaddr.unwrap() as *mut usize, 1024) };

      page_vaddr = mm::map_page_local(
        table,
        super::get_thread_local_virtual_base(),
        page_addr,
        self.map_count,
        false,
      );
    }

    if self.map_count == 0 {
      let mut pin_mask = AffinityMask::default();
      bits::set_bit(&mut pin_mask, idx);
      self.pin_mask = Some(pin_mask);
    }

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

    let idx = super::get_core_config().get_current_core_index();
    let local_base = super::get_thread_local_virtual_base() + (idx * super::get_section_size());
    let table_vaddr = super::get_page_virtual_address_for_virtual_address(local_base);
    let table = unsafe { slice::from_raw_parts_mut(table_vaddr.unwrap() as *mut usize, 1024) };

    mm::unmap_page_local(table, super::get_thread_local_virtual_base(), self.map_count);

    self.map_count -= 1;

    if self.map_count == 0 {
      self.pin_mask = None;
    }
  }
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
