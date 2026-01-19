//! AArch64 Task Tests

use crate::task::Task;
use core::slice;

/// Test local mappings.
///
/// # Description
///
/// For AArch64, this verifies that the thread-local mapping interface simply
/// returns linearly-mapped addresses.
pub fn run_local_mapping_tests() {
  let task = Task::get_current_task_mut();
  let kernel_vbase = crate::arch::get_kernel_virtual_base();

  // Map an address beyond 896 MiB. This address should be linearly mapped.
  let lcl_address = task.map_page(0x3900_0000);
  assert_eq!(lcl_address, 0x3900_0000 + kernel_vbase);

  // Write to the page. This will cause an exception if the mapping failed.
  let lcl_page =
    unsafe { slice::from_raw_parts_mut(lcl_address as *mut u8, crate::arch::get_page_size()) };
  lcl_page[0] = 42;
  assert_eq!(lcl_page[0], 42);

  // Remap the same page. The address should be the same.
  let lcl_address2 = task.map_page(0x3900_0000);
  assert_eq!(lcl_address2, lcl_address);

  // Map an address below 896 MiB. This address should be linearly mapped.
  let lcl_address3 = task.map_page(0x3700_0000);
  assert_eq!(lcl_address3, 0x3700_0000 + kernel_vbase);

  // Write to the page. This will cause an exception if the mapping failed.
  let lcl_page3 =
    unsafe { slice::from_raw_parts_mut(lcl_address3 as *mut u8, crate::arch::get_page_size()) };
  lcl_page3[0] = 42;
  assert_eq!(lcl_page3[0], 42);
}
