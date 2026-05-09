# contributing to pith

short version: send patches that read like the rest of the kernel.

## ground rules

- **stable rust only.** if a feature you want needs nightly, the answer
  is "wait or work around it". the only allowed unstable surface is
  the existing inline-asm intrinsics, which are stable.
- **no extra crates.** pith has zero dependencies and intends to keep
  it that way. if you reach for a crate, you're solving the wrong
  problem.
- **no allocator, no async, no surprises.** these are the three
  invariants the readme advertises. if your patch breaks any, it
  needs a separate discussion + a real motivation.

## what i'll merge fast

- new user-space tasks under `user/` that exercise existing syscalls.
- documentation patches in `docs/`.
- bug fixes with a reproducer in the commit message.
- smaller commits that don't touch the asm.

## what i'll review carefully

- changes to `kernel/src/trap.S` or `kernel/src/sched.S`. the asm /
  rust handoff is the most fragile boundary in the kernel; please
  pair the change with a smoke-boot test in qemu.
- new cap kinds. capability semantics carry forever; better to talk
  through the surface in an issue first.
- anything that adds a syscall. each one is a public abi commitment.

## ergonomics

- format with the project's default `rustfmt` (`cargo fmt`).
- keep lines under ~100 cols when possible. asm files and tables can
  go wider.
- comments use sentence case, lowercase first letter, no em-dashes.
  this is house style; sorry, no real reason.

## ci

every push runs `.github/workflows/ci.yml`: cargo build + a 15 s
qemu smoke-boot that grep-checks five gating phrases in the boot log.
if you break the build the CI badge in the readme tells everyone.

## reporting bugs

include:

- the exact `cargo run` command (almost always just `cargo run`).
- the boot log up to the moment the bug shows.
- the git sha of the kernel you're on (`git rev-parse --short HEAD`).
- your qemu version (`qemu-system-riscv64 --version`).

## license

contributions are MIT or Apache-2.0 at the contributor's discretion;
you choose, the project ships both.
