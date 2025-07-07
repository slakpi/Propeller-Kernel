//! Low-Level CPU Utilities

/// Halt the current core.
///
/// # Description
///
/// Halts the current core using an infinite wait loop.
.global cpu_halt
cpu_halt:
  brk     #0                // Trigger hardware breakpoint.
1:
  wfi                       // Use a wait to keep this from being a busy loop.
  b       1b                // Infinite loop.
