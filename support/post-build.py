# Post-build steps.
#
# NOTE: The llvm-tools and cargo-binutils components must be installed.
#
# NOTE: Requires Python 3.9+.

import argparse
import os
import subprocess

# Program main.
def main():
  args = parse_args()
  kernel = args.kernel[0]
  kernel_dir = os.path.dirname(kernel)
  kernel_img = os.path.join(kernel_dir, args.image)

  if args.assembly:
    asm_txt = os.path.join(kernel_dir, "asm.txt")

    # Dump the kernel assembly.
    with open(asm_txt, "w") as outfile:
      subprocess.run(["rust-objdump", "-h", "-D", kernel], stdout=outfile)

  # Create the binary image.
  ret = subprocess.run(["rust-objcopy", "-O", "binary", kernel, kernel_img])
  if ret.returncode != 0:
    raise RuntimeError("Failed to make kernel image.")


# Parse the command line arguments.
#
# Returns:
#     The argument namespace object.
def parse_args():
  parser = argparse.ArgumentParser(description="Propeller Kernel Runner")
  parser.add_argument("--image", required=True, help="The name of the kernel image file to create.")
  parser.add_argument("--assembly", action=argparse.BooleanOptionalAction, default=True,
                      help="Output a disassembly of the kernel image.")
  parser.add_argument("kernel", help="The kernel file to run.", nargs=1)
  return parser.parse_args()


# Program entry point.
if __name__ == "__main__":
  main()
