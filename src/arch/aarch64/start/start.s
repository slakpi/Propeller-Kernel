//! AArch64 Entry Point

.section ".text.boot"

/// Kernel entry point.
///
/// # Parameters
///
/// * w0 - 32-bit pointer to the ATAG/DTB blob (primary core)
/// * x1 - Zero
/// * x2 - Zero
/// * x3 - Zero
/// * x4 - Address of this entry point.
.global _start
_start:
  b       cpu_halt


.section ".text"
