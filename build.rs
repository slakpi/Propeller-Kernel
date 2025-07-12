use cc;
use std::env;

/// Files included in the AArch64 start library.
const AARCH64_START_FILES: [&'static str; 5] = [
  "src/arch/aarch64/start/cpu.s",
  "src/arch/aarch64/start/dtb.s",
  "src/arch/aarch64/start/exceptions.s",
  "src/arch/aarch64/start/mm.s",
  "src/arch/aarch64/start/start.s",
];

/// Build script entry.
fn main() {
  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

  let mut cfg = cc::Build::new();

  if target_arch == "aarch64" {
    configure_for_aarch64(&mut cfg);
  } else {
    assert!(false, "Invalid target architecture.");
  }

  cfg.compile("start");
}

/// Configure start library build for AArch64.
///
/// # Parameters
///
/// * `cfg` - The start library builder.
fn configure_for_aarch64(cfg: &mut cc::Build) {
  cfg
    .include("src/arch/aarch64/start/include")
    .files(&AARCH64_START_FILES);

  for file in &AARCH64_START_FILES {
    println!("cargo:rerun-if-changed={}", file);
  }
}
