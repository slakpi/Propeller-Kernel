//! Symmetric Multiprocessing

use crate::arch::{self, cpu};
use crate::debug_print;
use crate::io;
use crate::mm;
use crate::support::bits;
use crate::support::core_config::{Core, CoreEnableMethod};
use crate::support::memory::MemoryZone;
use crate::task::Task;

/// Re-initialization guard.
static mut INITIALIZED: bool = false;

/// Initialize the symmetric multiprocessing module.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
pub fn init() {
  unsafe {
    assert!(!INITIALIZED);
    INITIALIZED = true;
  }

  debug_print!("--- SMP Initialization ---\n");

  // Architecture-dependent SMP initialization.
  let alloc = mm::get_zone_allocator(MemoryZone::LinearMemoryZone)
    .as_ref()
    .unwrap();
  let start_addr = arch::smp_init(alloc.lock().as_mut());

  // Release the cores.
  let core_config = arch::get_device_tree().get_core_config();
  let enable_method = core_config.get_enable_method();

  debug_print!("Releasing secondary cores...\n");

  for core in core_config.get_cores().iter().skip(1) {
    release_core(core, start_addr, enable_method);
  }
}

/// Release a core using its enable method.
///
/// # Parameters
///
/// * `core` - The core to release.
/// * `start_addr` - The address at which the core should start.
/// * `enable_method` - The enable method to use.
///
/// # Description
///
/// If the enable method is None, the enable methods specified by the core
/// will be used.
fn release_core(core: &Core, start_addr: usize, enable_method: Option<CoreEnableMethod>) {
  let enable_method = if let Some(enable_method) = enable_method {
    enable_method
  } else if let Some(enable_method) = core.enable_method {
    enable_method
  } else {
    panic!("Invalid enable method.");
  };

  match enable_method {
    CoreEnableMethod::SpinTable => release_via_spin_table(core, start_addr),
    CoreEnableMethod::Bcm2836 => release_via_bcm2836(core, start_addr),
  }
}

/// Release a core using a spin table.
///
/// # Parameters
///
/// * `core` - The core to release.
/// * `start_addr` - The address at which the core should start.
fn release_via_spin_table(core: &Core, start_addr: usize) {
  // The release address is in physical memory, but not guaranteed to be
  // mapped linearly on all architectures.
  let task = Task::get_current_task_mut();
  let phys_page = bits::align_down(core.release_addr, arch::get_page_size());
  let offset = core.release_addr - phys_page;
  let virt_addr = (task.map_page(phys_page) + offset);

  unsafe {
    let ptr = virt_addr as *mut usize;
    *ptr = start_addr;
  }

  task.unmap_page();
  cpu::flush_data_cache_by_va(virt_addr);
  cpu::data_sync_barrier();
  cpu::send_event();
}

/// Release a core using the BCM2836 mailboxes.
///
/// # Parameters
///
/// * `core` - The core to release.
/// * `start_addr` - The address at which the core should start.
fn release_via_bcm2836(core: &Core, start_addr: usize) {
  // TODO: Need to get the physical base address from the device tree.
  const BCM2836_PHYS_ADDR: usize = 0x4000_0000;
  const OFFSETS: [usize; 4] = [0x8c, 0x9c, 0xac, 0xbc];

  // The BCM2836 only has four cores and the lower two bits of their
  // identifiers need to be set to correspond to the mailbox numbers.
  let index = core.id & 0x3;

  let Some(virt_addr) = io::map_io(BCM2836_PHYS_ADDR, 1) else {
    return;
  };

  unsafe {
    let ptr = (virt_addr + OFFSETS[index]) as *mut usize;
    *ptr = start_addr;
  }

  io::unmap_io(virt_addr);
  cpu::send_event();
}
