//! AArch64 Task Management

unsafe extern "C" {
  fn task_get_current_task_addr() -> usize;
  fn task_set_current_task_addr(task: usize);
}

/// AArch64 CPU register context.
///
///   TODO: Add floating-point registers for user tasks.
pub struct TaskContext {
  x19: usize,
  x20: usize,
  x21: usize,
  x22: usize,
  x23: usize,
  x24: usize,
  x25: usize,
  x26: usize,
  x27: usize,
  x28: usize,
  x29: usize, // the frame pointer
  x30: usize, // the link register
  sp: usize,  // the stack pointer
}

impl TaskContext {
  /// Construct a zeroed task context.
  pub const fn new() -> Self {
    TaskContext {
      x19: 0,
      x20: 0,
      x21: 0,
      x22: 0,
      x23: 0,
      x24: 0,
      x25: 0,
      x26: 0,
      x27: 0,
      x28: 0,
      x29: 0,
      x30: 0,
      sp: 0,
    }
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

/// Maps a page into the kernel's virtual address space.
///
/// # Parameters
///
/// * `context` - The task's context.
/// * `page_addr` - The physical address of the page to map.
/// * `device` - Whether this page maps to device memory.
///
/// # Description
///
///   NOTE: This function exists to satisfy the Task interface requirements and
///         simply adds the physical page address to the virtual base under the
///         assumption that all physical memory has been linearly mapped into
///         the kernel's virtual address space.
///
/// # Returns
///
/// The virtual address of the mapped page.
pub fn map_page_local(_context: &mut TaskContext, page_addr: usize, _device: bool) -> usize {
  super::get_kernel_config().virtual_base + page_addr
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
pub fn unmap_page_local(_context: &mut TaskContext) {}
