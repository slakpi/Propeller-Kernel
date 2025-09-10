//! ARM Task Management

use super::mm;
use core::ptr;

unsafe extern "C" {
  fn task_get_current_task_addr() -> usize;
  fn task_set_current_task_addr(task: usize);
}

/// The maximum number of local mappings a task can maintain.
const MAX_LOCAL_MAPPINGS: usize = 512;

/// Helper type to force alignment of the local mapping table. The compiler will
/// ensure the local mapping table is aligned to a page boundary and rearrange
/// the remaining fields of the task structure accordingly.
#[repr(C, align(4096))]
struct AlignedTable([usize; 1024]);

/// ARMv7a CPU register and memory context.
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
  mappings: AlignedTable,
  map_count: usize,
}

impl TaskContext {
  /// Construct a zeroed task context.
  pub const fn new() -> Self {
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
      mappings: AlignedTable([0; 1024]),
      map_count: 0,
    }
  }

  /// Get the address of the local mapping table.
  pub fn get_local_mapping_table_addr(&self) -> usize {
    unsafe { ptr::addr_of!(self.mappings) as usize }
  }
}

/// Get the current task address.
pub fn get_current_task_addr() -> usize {
  unsafe { task_get_current_task_addr() }
}

/// Set the current task address.
pub fn set_current_task_addr(task: usize) {
  unsafe { task_set_current_task_addr(task) };
}

/// Maps a page into the kernel's virtual address space using the current task's
/// local mapping table.
///
/// # Parameters
///
/// * `context` - The task's context.
/// * `page_addr` - The physical address of the page to map.
/// * `device` - Whether this page maps to device memory.
///
/// # Description
///
/// If the page is in low memory, adds a null entry to the local mapping table
/// and returns the virtual address of the linearly mapped page. This means that
/// pages in low memory still count toward the number of local mappings a task
/// is maintaining.
///
///   NOTE: This is done to simplify the unmapping logic which does not take an
///         address parameter.
///
/// Otherwise, if the page is in high memory, the function maps the page to the
/// next available virtual address in the task's local mappings. The mappings
/// are thread-local, so the function is thread safe.
///
///   TODO: The Linux implementation ensures the thread is pinned to the same
///         CPU for the duration of temporary mappings. That is necessary to
///         ensure mappings are consistent if the thread is preempted.
///
/// The function will panic if no more pages can be mapped into the thread's
/// local mappings.
///
/// # Returns
///
/// The virtual address of the mapped page.
pub fn map_page_local(context: &mut TaskContext, page_addr: usize, device: bool) -> usize {
  let page_vaddr: usize;

  assert!(context.map_count < MAX_LOCAL_MAPPINGS);

  page_vaddr = mm::map_page_local(
    &mut context.mappings.0,
    super::get_thread_local_virtual_base(),
    page_addr,
    context.map_count,
    device,
  );

  context.map_count += 1;
  page_vaddr
}

/// Unmaps the previously mapped page in the current task's local mapping
/// table.
///
/// # Parameters
///
/// * `context` - The task's context.
///
/// # Description
///
///   NOTE: This function exists to satisfy the Task interface requirements and
///         does nothing.
pub fn unmap_page_local(context: &mut TaskContext) {
  if context.map_count == 0 {
    return;
  }

  mm::unmap_page_local(
    &mut context.mappings.0,
    super::get_thread_local_virtual_base(),
    context.map_count,
  );

  context.map_count -= 1;
}
