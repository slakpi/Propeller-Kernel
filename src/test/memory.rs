//! Test Memory Utilities

use crate::support::bits;

/// Test page size.
pub const PAGE_SIZE: usize = 4096;

/// Test page shift.
pub const PAGE_SHIFT: usize = bits::floor_log2(PAGE_SIZE);

/// Provide 9 MiB of test memory.
pub const PAGE_COUNT: usize = 2304;

/// Total memory available in bytes.
pub const MEMORY_SIZE: usize = PAGE_SIZE * PAGE_COUNT;

/// Alignment type.
#[repr(align(0x400000))]
struct _Align4MiB;

/// Wrapper type to align the memory block.
struct _MemWrapper {
  _alignment: [_Align4MiB; 0],
  mem: [u8; MEMORY_SIZE],
}

/// Statically allocate the test memory as part of the kernel image.
static mut TEST_MEM: _MemWrapper = _MemWrapper {
  _alignment: [],
  mem: [0xcc; MEMORY_SIZE],
};

/// Get the static memory block.
pub fn get_test_memory_mut() -> &'static mut [u8] {
  unsafe { &mut (*(&raw mut TEST_MEM)).mem }
}

/// Reset the contents of the static memory block.
pub fn reset_test_memory() {
  let mem = get_test_memory_mut();
  mem.fill(0xcc);
}
