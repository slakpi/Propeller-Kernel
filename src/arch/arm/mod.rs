//! ARM Architecture

/// ARM platform configuration.
///
/// # Parameters
///
/// * `config` - The kernel configuration address provided by the start code.
///
/// # Description
///
///   NOTE: Must only be called once while the kernel is single-threaded.
///
///   NOTE: Assumes 4 KiB pages.
///
///   NOTE: Assumes the kernel stack page count is a power of two.
///
///   NOTE: Assumes the blob is a DTB.
pub fn init(_config: usize) {}
