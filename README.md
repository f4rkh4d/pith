# pith

a small, sober, sel4-shaped microkernel for `riscv64`.

```
no async. no allocator. no surprises.
```

5 minutes of fame:

```
pith v0.1.0
hart 0 booting on rv64

[pith] paging on (sv39)
[pith] trap vector installed
[pith] proc init: 16 bytes user binary
[pith] entering userspace
[user]  hello from u-mode (via ecall)
[pith] user exited cleanly
```

## what it is

a kernel that boots on `qemu-system-riscv64`, maps its own pages with sv39,
installs a trap vector, drops the cpu into u-mode running a user task,
services that task's `ecall`s, and shuts down cleanly when the user exits.

every byte you see in the boot log is produced by code in this repo. there
are no firmware blobs we don't understand, no third-party kernel libraries,
no tooling that hides what is happening.

## what it is not (yet)

- not multi-task. v0.1 runs a single user binary. v0.2 wires the timer
  interrupt + a runqueue.
- not capability-gated. the data structures (`ipc::Endpoint`,
  `ipc::Message`) are in tree, the syscalls aren't. v0.3.
- not multi-hart. boot stub parks every hart that isn't `0`. v0.4 brings
  per-hart stacks + the `hart_start` SBI flow.
- not for real hardware. qemu-virt is the only target. real boards in v1.0
  once the pieces above settle.

if any of that disappoints you, you are looking for a more finished kernel.
this one is being built in public.

## quickstart

```sh
# requirements: rustup with the riscv64gc-unknown-none-elf target,
# qemu-system-riscv64 8.0+ (10.x recommended).

rustup target add riscv64gc-unknown-none-elf
brew install qemu          # or your distro's package

cargo run
```

press `ctrl-a x` to detach from qemu when the kernel is wedged.

## layout

```
pith/
├── kernel/
│   ├── src/
│   │   ├── main.rs       # kmain. boots, hands off to the rest.
│   │   ├── boot.S        # opensbi -> _start. zeroes bss, sets sp.
│   │   ├── trap.S        # one trap vector for s-mode and u-mode.
│   │   ├── trap.rs       # trap_dispatch: scause -> handler.
│   │   ├── linker.ld     # image at 0x80200000, kernel-end stack.
│   │   ├── uart.rs       # 16550 mmio at 0x10000000.
│   │   ├── sbi.rs        # opensbi shutdown + timer.
│   │   ├── mm.rs         # page bump + sv39 page table builder.
│   │   ├── proc.rs       # the first user task. embeds 16 bytes.
│   │   ├── ipc.rs        # endpoints + messages. v0.2 wires send/recv.
│   │   └── syscall.rs    # ecall dispatch. exit, hi, putc, yield.
│   └── Cargo.toml
├── scripts/
│   └── run.sh            # qemu wrapper, used as cargo's runner.
├── docs/                 # tutorial-tier walk-through, one chapter
│   ├── 01-boot.md        # per concept. start here if you want to
│   ├── 02-uart.md        # follow what each commit changes.
│   ├── 03-traps.md
│   ├── 04-paging.md
│   ├── 05-userspace.md
│   └── 06-ipc.md
├── .cargo/config.toml
├── rust-toolchain.toml
└── Cargo.toml
```

## design notes

**boot path is two files.**
[boot.S](kernel/src/boot.S) is the only assembly that runs before any rust
code. it parks non-bsp harts in `wfi`, points `sp` at `__stack_top`, zeros
bss, and falls through to `kmain` in [main.rs](kernel/src/main.rs). that
is the entire ceremony.

**one trap vector.**
[trap.S](kernel/src/trap.S) is the single entry for both kernel-mode and
user-mode traps. on entry it swaps `sscratch` with `sp`. if the swap
yields zero we were already on the kernel stack (kernel-from-kernel trap);
otherwise `sscratch` held the kernel stack top and `sp` is now pointing
at fresh kernel memory. either way, we save 32 GP registers + sepc +
sstatus into a `TrapFrame` and call `trap_dispatch` in
[trap.rs](kernel/src/trap.rs).

**sv39 only.**
[mm.rs](kernel/src/mm.rs) builds a 3-level page table that identity-maps
the uart, the PLIC, and ram, then turns paging on with one `csrw satp`
+ `sfence.vma`. the user task's pages live above the identity range
(`0x40000000+`) with `PTE_U` set. the kernel side has no `PTE_U` and is
inaccessible from u-mode.

**no allocator beyond pages.**
`alloc_page` is a single bump allocator that hands out 4 KiB chunks above
`__kernel_end`. the page tables, the user pages, and the kernel stacks all
come out of the same pool. no `Box`, no `Vec`, no `&mut dyn Trait`. when
you see `static mut FIRST: Process` in [proc.rs](kernel/src/proc.rs) it
is intentional: we run with one task and one hart, so the lock you would
otherwise need does not exist.

**syscall abi.**
ECALL trap from u-mode lands in [syscall::dispatch](kernel/src/syscall.rs)
with `a7 = number, a0..a5 = args, a0 = return`. the four syscalls in v0.1
are deliberate: EXIT(0), HI(1), PUTC(2), YIELD(3). HI is here so the very
first proof-of-life does not depend on a working user pointer; in v0.2
WRITE(buf, len) replaces it.

**ipc surface is locked.**
the v0.1 [ipc](kernel/src/ipc.rs) module has the data types + the doc
comments. the actual send/recv is the v0.2 work. shipping the API
shape early so future reviewers can argue with the surface, not the
implementation.

## roadmap

- v0.2: timer interrupt, cooperative preemption, two user tasks, real
  WRITE syscall, sweet user crate with build.rs.
- v0.3: capability table. endpoints accessed by handle, not by integer.
  send/recv lit up.
- v0.4: hart_start SBI flow, per-hart kernel stack, big lock around the
  scheduler, then a fine-grained one.
- v0.5: device-tree parser, real memory probe, drop the `PHYS_END`
  constant. virtio-blk + tiny fs.
- v1.0: boots on a real visionfive 2.

## why call it `pith`

the part of a stem that everything else hangs off. the tiniest piece you
can take out and still have a tree. fitting for a kernel that intends to
stay small.

## license

MIT or Apache 2.0, your pick.
