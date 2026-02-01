//! AArch64 Task Tests

use crate::task::Task;
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
/// For AArch64, this verifies that the thread-local mapping interface simply
/// returns linearly-mapped addresses.
fn run_local_mapping_tests(context: &mut test::TestContext) {
  let task = Task::get_current_task_mut();
  let virt_base = crate::arch::get_kernel_virtual_base();

  // Map an address beyond 896 MiB. This address should be linearly mapped.
  let lcl_address = task.map_page(0x3900_0000);
  check_eq!(context, lcl_address, 0x3900_0000 + virt_base);

  // Write to the page. This will cause an exception if the mapping failed.
  let lcl_page =
    unsafe { slice::from_raw_parts_mut(lcl_address as *mut u8, crate::arch::get_page_size()) };
  lcl_page[0] = 42;
  check_eq!(context, lcl_page[0], 42);

  // Remap the same page. The address should be the same.
  let lcl_address2 = task.map_page(0x3900_0000);
  check_eq!(context, lcl_address2, lcl_address);

  // Map an address below 896 MiB. This address should be linearly mapped.
  let lcl_address3 = task.map_page(0x3700_0000);
  check_eq!(context, lcl_address3, 0x3700_0000 + virt_base);

  // Write to the page. This will cause an exception if the mapping failed.
  let lcl_page3 =
    unsafe { slice::from_raw_parts_mut(lcl_address3 as *mut u8, crate::arch::get_page_size()) };
  lcl_page3[0] = 42;
  check_eq!(context, lcl_page3[0], 42);
}
