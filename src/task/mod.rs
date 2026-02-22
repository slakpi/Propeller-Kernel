//! Task Management

pub use crate::arch::task::*;

use crate::debug_print;
use core::ptr;

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// The bootstrap task is a special task that exists only to provide a way to
/// manage high memory mappings before the kernel allocators and scheduler are
/// initialized. The bootstrap task will only be used by the primary core.
///
/// Once the kernel maps system memory, initializes the kernel allocators,
/// initializes the scheduler, and enables the secondary cores, the bootstrap
/// task will be replaced by the real init thread tasks.
static mut BOOTSTRAP_TASK: Task = Task::new(0, TaskContext::default());

/// The architecture-independent task object.
///
/// The architecture must implement the TaskContext object for architecture-
/// dependent operations.
pub struct Task {
  task_id: usize,
  affinity: Option<AffinityMask>,
  context: TaskContext,
}

impl Task {
  /// Construct a new task.
  ///
  /// # Parameters
  ///
  /// * `task_id` - The new task's identifier.
  pub const fn new(task_id: usize, context: TaskContext) -> Self {
    Task {
      task_id,
      affinity: None,
      context,
    }
  }

  /// Get a reference to the current task.
  pub fn get_current_task<'task>() -> &'task Task {
    Task::get_current_task_mut()
  }

  /// Get a mutable reference to the current task.
  pub fn get_current_task_mut<'task>() -> &'task mut Task {
    let addr = unsafe { get_current_task_addr() };

    assert_ne!(addr, 0);
    unsafe { &mut *(addr as *mut Task) }
  }

  /// Set the current task.
  ///
  /// # Parameters
  ///
  /// * `task` - The task that will begin running.
  pub fn set_current_task(task: &Task) {
    unsafe { set_current_task_addr(task as *const _ as usize) };
  }

  /// Get the task identifier.
  pub fn get_task_id(&self) -> usize {
    self.task_id
  }

  /// The task's core affinity mask.
  pub fn get_affinity(&self) -> Option<&AffinityMask> {
    match self.context.get_pin_mask() {
      Some(pin_mask) => Some(pin_mask),
      None => self.affinity.as_ref(),
    }
  }

  /// Set the task's core affinity mask.
  ///
  /// # Parameters
  ///
  /// * `affinity` - The new affinity mask or None to allow running on any core.
  pub fn set_affinity(&mut self, affinity: Option<&AffinityMask>) {
    if let Some(affinity) = affinity {
      self.affinity = Some(*affinity);
    } else {
      self.affinity = None;
    }
  }

  /// Get a reference to the task's architecture-dependent context.
  pub fn get_context(&self) -> &TaskContext {
    &self.context
  }

  /// Get a mutable reference to the task's architecture-dependent context.
  pub fn get_context_mut(&mut self) -> &mut TaskContext {
    &mut self.context
  }

  /// Maps a page into the kernel's address space.
  ///
  /// # Parameters
  ///
  /// * `page_addr` - The physical address of the page to map.
  ///
  /// # Description
  ///
  /// Thread-local mappings follow stack semantics. The first page mapped will
  /// be the last page unmapped and vice versa for the last page mapped. Thread-
  /// local mappings should not be maintained beyond the current context.
  ///
  /// The function will panic if no more pages can be added to the thread's
  /// mapping table.
  ///
  ///   NOTE: Only 32-bit architectures implement thread-local mapping, but this
  ///         interface should be used for architecture independence. On a
  ///         64-bit architecture, the compiler will optimize the map call down
  ///         to a simple addition (virtual base + page physical address).
  ///
  /// # Returns
  ///
  /// The virtual address of the mapped page.
  pub fn map_page(&mut self, page_addr: usize) -> usize {
    self.context.map_page(page_addr)
  }

  /// Unmaps the last mapped page in the current task's local mapping table.
  ///
  ///   NOTE: Only 32-bit architectures implement thread-local mapping, but this
  ///         interface should be used for architecture independence. On a
  ///         64-bit architecture, the compiler will optimize away the unmap
  ///         call.
  pub fn unmap_page(&mut self) {
    self.context.unmap_page();
  }
}

/// Initialize the task module and the bootstrap task.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
pub fn init() {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  // There is no need to go through the normal context switch process. The
  // bootstrap task is technically already "running." We only need to set the
  // running task pointer on the primary core and map the task's local mapping
  // table into the kernel page tables.
  let task = unsafe { ptr::addr_of_mut!(BOOTSTRAP_TASK).as_mut().unwrap() };
  *task = Task::new(0, init_bootstrap_context());

  // Update the current task pointer.
  Task::set_current_task(task);
  
  debug_print!("task init complete.\n");
}
