//! ARM Task Tests

use crate::task::{Task, TaskContext};
use crate::{check_eq, execute_test, test};
use core::slice;

/// Run task tests.
///
/// # Parameters
///
/// * `context` - The test context.
pub fn run_tests(context: &mut test::TestContext) {
  execute_test!(context, run_local_mapping_tests);
}

/// Test local mappings.
///
/// # Parameters
///
/// * `context` - The test context.
///
/// # Description
///
/// For ARM, this verifies that the thread-local mapping interface maps pages
/// into the thread's table and returns addresses in the core's mapping range.
fn run_local_mapping_tests(context: &mut test::TestContext) {
  let task = Task::get_current_task_mut();
  let virt_base = crate::arch::get_kernel_virtual_base();
  let page_size = crate::arch::get_page_size();
  let page_mask = crate::arch::get_page_mask();
  let local_vbase = TaskContext::get_thread_local_virtual_base(0);
  let table_vaddr = TaskContext::get_page_virtual_address_for_virtual_address(local_vbase);
  let table = unsafe { slice::from_raw_parts_mut(table_vaddr.unwrap() as *mut usize, 1024) };

  // Map an address beyond 896 MiB; assuming we are running on the primary core.
  let lcl_address = task.map_page(0x3900_0000);
  check_eq!(context, lcl_address, local_vbase);
  check_eq!(context, task.get_context().map_count, 1);
  check_eq!(context, table[0] & !page_mask, 0x3900_0000);
  check_eq!(context, table[1], 0);

  // Write to the page. This will cause an exception if the mapping failed.
  let lcl_page = unsafe { slice::from_raw_parts_mut(lcl_address as *mut u8, page_size) };
  lcl_page[0] = 42;
  check_eq!(context, lcl_page[0], 42);

  // Remap the same page; verify the address increments by a page.
  let lcl_address2 = task.map_page(0x3900_0000);
  check_eq!(context, lcl_address2, lcl_address + page_size);
  check_eq!(context, task.get_context().map_count, 2);
  check_eq!(context, table[2] & !page_mask, 0x3900_0000);
  check_eq!(context, table[3], 0);

  // Write to the page. Verify the change is seen through both slices.
  let lcl_page2 = unsafe { slice::from_raw_parts_mut(lcl_address2 as *mut u8, page_size) };
  lcl_page2[0] = 21;
  check_eq!(context, lcl_page2[0], 21);
  check_eq!(context, lcl_page2[0], lcl_page[0]);

  // Map an address below 896 MiB. This address should be linearly mapped.
  let lcl_address3 = task.map_page(0x3700_0000);
  check_eq!(context, lcl_address3, 0x3700_0000 + virt_base);
  check_eq!(context, task.get_context().map_count, 3);
  check_eq!(context, table[4], 0);
  check_eq!(context, table[5], 0);

  task.unmap_page();
  check_eq!(context, task.get_context().map_count, 2);
  check_eq!(context, table[4], 0);

  task.unmap_page();
  check_eq!(context, task.get_context().map_count, 1);
  check_eq!(context, table[2], 0);

  task.unmap_page();
  check_eq!(context, task.get_context().map_count, 0);
  check_eq!(context, table[0], 0);
}
