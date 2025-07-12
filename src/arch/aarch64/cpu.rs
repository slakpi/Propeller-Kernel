unsafe extern "C" {
  fn cpu_halt() -> !;
}

pub fn halt() -> ! {
  unsafe { cpu_halt() };
}
