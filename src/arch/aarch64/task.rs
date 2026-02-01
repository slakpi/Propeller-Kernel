//! AArch64 Task Management

#[cfg(feature = "module_tests")]
mod tests;

use crate::arch::cpu;
use crate::support::bits;
#[cfg(feature = "module_tests")]
use crate::test;

unsafe extern "C" {
  fn task_get_current_task_addr() -> usize;
  fn task_set_current_task_addr(task: usize);
}

const CPU_MASK_WORDS: usize = (cpu::MAX_CORES + usize::BITS as usize - 1) / usize::BITS as usize;

pub type AffinityMask = bits::Bitmap<CPU_MASK_WORDS>;

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// AArch64 task context.
///
///   TODO: Add floating-point registers for user tasks.
///
///   TODO: Implement context switching.
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
  /// Construct an empty task context.
  pub const fn default() -> Self {
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

  /// Construct a new task context.
  pub fn new() -> Self {
    Self::default()
  }

  /// Get the current pin mask.
  pub fn get_pin_mask(&self) -> Option<&AffinityMask> {
    None
  }

  /// See `Task::map_page()`.
  ///
  /// # Parameters
  ///
  /// * `page_addr` - The physical address of the page to map.
  ///
  /// # Description
  ///
  ///   NOTE: This function exists to satisfy the TaskContext interface
  ///         requirements and simply adds the physical page address to the
  ///         virtual base under the assumption that all physical memory has
  ///         been linearly mapped into the kernel's virtual address space.
  ///
  /// # Returns
  ///
  /// The virtual address of the mapped page.
  pub fn map_page(&mut self, page_addr: usize) -> usize {
    super::get_kernel_virtual_base() + page_addr
  }

  /// See `Task::unmap_page()`.
  ///
  /// # Description
  ///
  ///   NOTE: This function exists to satisfy the TaskContext interface
  ///         requirements and does nothing.
  pub fn unmap_page(&mut self) {}
}

/// Initialize the bootstrap task context.
///
/// # Description
///
///   NOTE: This function exists to satisfy the TaskContext interface
///         requirements and simply returns an empty task context.
pub fn init_bootstrap_context() -> TaskContext {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  TaskContext::new()
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

#[cfg(feature = "module_tests")]
pub fn run_tests(context: &mut test::TestContext) {
  tests::run_tests(context);
}
