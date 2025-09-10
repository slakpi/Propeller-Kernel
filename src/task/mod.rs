//! Task Management

use crate::arch::task;

/// Architecture-independent task object.
pub struct Task {
  task_id: usize,
  affinity: Option<usize>,
  context: task::TaskContext,
}

impl Task {
  /// Construct a new task object with an architecture context.
  ///
  /// # Parameters
  ///
  /// * `task_id` - The new task's identifier.
  ///
  /// # Returns
  ///
  /// A new task object.
  pub const fn new(task_id: usize) -> Self {
    Task {
      task_id,
      affinity: None,
      context: task::TaskContext::new(),
    }
  }

  /// Get a reference to the current task.
  pub fn get_current_task<'task>() -> &'task Self {
    Self::get_current_task_mut()
  }

  /// Get a mutable reference to the current task.
  pub fn get_current_task_mut<'task>() -> &'task mut Self {
    let addr = task::get_current_task_addr();
    assert_ne!(addr, 0);
    unsafe { &mut *(addr as *mut Self) }
  }

  /// Temporarily map a page into the kernel's virtual address space.
  ///
  /// # Parameters
  ///
  /// * `page_addr` - The physical address of the page to map.
  /// * `device` - Whether this page maps to device memory.
  ///
  /// # Description
  ///
  /// Rules:
  ///
  /// * Depending on the platform, the mapped address may only be valid on a
  ///   single core. As such, these platforms will pin the task to the core
  ///   currently running the task until all local mappings have been unmapped.
  ///   Holding on to local mappings can greatly affect performance.
  ///
  /// * Depending on the platform, the number of local mappings allowed may be
  ///   constrained. Holding on to local mappings can limit the ability for a
  ///   task to map pages.
  ///
  /// * Local mappings follow stack semantics. The first page mapped locally
  ///   *must* be the last page unmapped. This will complicate attempts to hold
  ///   on to local mappings.
  ///
  /// # Returns
  ///
  /// The virtual address of the mapped page.
  pub fn map_page_local(page_addr: usize, device: bool) -> usize {
    let cur_task = Self::get_current_task_mut();
    task::map_page_local(cur_task.get_context_mut(), page_addr, device)
  }

  /// Unmap the previously mapped local page from the kernel's address space.
  ///
  /// # Description
  ///
  /// Local mappings follow stack semantics. As such, this function may only be
  /// used to unmap the last page mapped. If no pages have been mapped locally,
  /// this function does nothing.
  pub fn unmap_page_local() {
    let cur_task = Self::get_current_task_mut();
    task::unmap_page_local(cur_task.get_context_mut());
  }

  /// Get the task's identifier.
  pub fn get_task_id(&self) -> usize {
    self.task_id
  }

  /// Get the task's core affinity.
  pub fn get_affinity(&self) -> Option<usize> {
    self.affinity
  }

  /// Get the task's architecture context.
  ///
  /// # Description
  ///
  ///   NOTE: The task context is effectively an opaque type to any architecture
  ///         independent code. The interface is not consistent across
  ///         architectures.
  pub fn get_context(&self) -> &task::TaskContext {
    &self.context
  }

  /// Get the task's architecture context.
  ///
  /// # Description
  ///
  ///   NOTE: The task context is effectively an opaque type to any architecture
  ///         independent code. The interface is not consistent across
  ///         architectures.
  pub fn get_context_mut(&mut self) -> &mut task::TaskContext {
    &mut self.context
  }
}
