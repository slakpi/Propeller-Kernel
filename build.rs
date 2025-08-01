use cc;
use std::env;

/// Build script entry.
fn main() {
  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

  let mut cfg = cc::Build::new();

  // None of the default flags are needed. The start library is assembly-only,
  // so optimization has no effect. The cargo configuration files will set the
  // required architecture and CPU flags.
  cfg.no_default_flags(true);

  if target_arch == "aarch64" {
    configure_for_aarch64(&mut cfg);
  } else if target_arch == "arm" {
    configure_for_arm(&mut cfg);
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
  const AARCH64_START_FILES: [&'static str; 5] = [
    "src/arch/aarch64/start/cpu.s",
    "src/arch/aarch64/start/dtb.s",
    "src/arch/aarch64/start/exceptions.s",
    "src/arch/aarch64/start/mm.s",
    "src/arch/aarch64/start/start.s",
  ];

  cfg
    .include("src/arch/aarch64/start/include")
    .files(&AARCH64_START_FILES);

  println!("cargo:rerun-if-changed=src/arch/aarch64/start/start.ld");

  for file in &AARCH64_START_FILES {
    println!("cargo:rerun-if-changed={}", file);
  }
}

/// Configure start library build for ARM.
///
/// # Parameters
///
/// * `cfg` - The start library builder.
fn configure_for_arm(cfg: &mut cc::Build) {
  const ARM_START_FILES: [&'static str; 6] = [
    "src/arch/arm/start/cpu.s",
    "src/arch/arm/start/dtb.s",
    "src/arch/arm/start/exceptions.s",
    "src/arch/arm/start/extensions.s",
    "src/arch/arm/start/mm.s",
    "src/arch/arm/start/start.s",
  ];

  cfg
    .include("src/arch/arm/start/include")
    .files(&ARM_START_FILES);

  println!("cargo:rerun-if-changed=src/arch/arm/start/start.ld");

  for file in &ARM_START_FILES {
    println!("cargo:rerun-if-changed={}", file);
  }
}
