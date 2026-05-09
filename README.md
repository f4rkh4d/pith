# pith

[![ci](https://github.com/f4rkh4d/pith/actions/workflows/ci.yml/badge.svg)](https://github.com/f4rkh4d/pith/actions/workflows/ci.yml)

a small, sober, sel4-shaped microkernel for `riscv64`.

```
no async. no allocator. no surprises.
```

## numbers

measured on qemu-virt (rv64, 10 MHz timebase, single hart, opensbi default
firmware) by [user/bench/](user/bench/) running against [user/mirror/](user/mirror/):

| operation                    | n     | total cycles | avg cyc/op |
|------------------------------|------:|-------------:|-----------:|
| empty syscall (SYS_YIELD)    | 10000 |    7 118 000 |    **711** |
| ipc round-trip (4 ipc ops)   |  5000 |   73 754 000 | **14 750** |

"cycle" on qemu-virt ticks at the mtime rate (10 MHz), so divide by 10 to
get nanoseconds: 71 ns per syscall, 1.5 µs per round-trip. on real silicon
at ~1 GHz the same code paths land roughly at 200 ns / 4 µs.

## architecture

```
              ┌──────────────────────────────────────────────┐
              │  u-mode                                      │
  ┌──────┐    │  ┌────────┐  ┌────────┐  ┌────────┐          │
  │ user │    │  │  ping  │  │ ping2  │  │  pong  │  …       │
  │ apps │    │  │ Cap[16]│  │ Cap[16]│  │ Cap[16]│          │
  └──────┘    │  │ stack  │  │ stack  │  │ stack  │          │
              │  │ pt sv39│  │ pt sv39│  │ pt sv39│          │
              │  └───┬────┘  └───┬────┘  └───┬────┘          │
              └──────┼───────────┼───────────┼───────────────┘
                     │ ecall     │ ecall     │ ecall
              ┌──────┼───────────┼───────────┼───────────────┐
              │  s-mode (pith)   ▼           ▼               │
              │  ┌──────────────────────────────────┐        │
              │  │ trap.S  -> trap_dispatch          │        │
              │  └──────────┬─────────────┬─────────┘        │
              │             │             │                  │
              │             ▼             ▼                  │
              │  ┌─────────────┐  ┌─────────────────┐        │
              │  │ syscall.rs  │  │ timer (10 ms)   │        │
              │  │  EXIT YIELD │  │ sbi::set_timer  │        │
              │  │  WRITE PUTC │  └─────┬───────────┘        │
              │  │  SEND RECV  │        │                    │
              │  │  CAP_DUPE   │        ▼                    │
              │  │  CAP_DELETE │  ┌─────────────────┐        │
              │  └──────┬──────┘  │  sched.rs       │        │
              │         │         │  proc table     │        │
              │         ▼         │  KContext       │        │
              │  ┌─────────────┐  │  context_switch │        │
              │  │  ipc.rs     │◀─┤  install_runtime│        │
              │  │  endpoints  │  │  yield_now      │        │
              │  │  fifo wq    │  │  block / wake   │        │
              │  └──────┬──────┘  └─────────┬───────┘        │
              │         │                   │                │
              │         ▼                   ▼                │
              │  ┌─────────────────────────────────┐         │
              │  │  mm.rs: sv39 paging + bump page │         │
              │  │  uart.rs (16550 mmio)           │         │
              │  │  sbi.rs (timer + shutdown)      │         │
              │  └─────────────────────────────────┘         │
              └──────────────────────────────────────────────┘
                                  │
                          ┌───────▼────────┐
                          │ opensbi (m-mode)│
                          └────────────────┘
```

## 5 minutes of fame

```
pith v0.8.0
hart 0 booting on rv64

[pith] paging on (sv39)
[pith] trap vector installed (timer quantum = 100000 ticks)
[sched] spawned bench  as pid 1 (4389 bytes)
[sched] spawned mirror as pid 2 (70 bytes)
[pith] endpoints ep_a=#0 ep_b=#1 wired to bench (pid 1) and mirror (pid 2)
[pith] entering scheduler
bench  starting (qemu-virt, rv64, 10 MHz time base)
bench  syscall (YIELD): total=7118000 cycles, n=10000, avg=711 cyc/op
bench  ipc round-trip : total=73754000 cycles, n=5000, avg=14750 cyc/op
bench  done
[sched] pid 1 exited (code 0)
[sched] pid 2 exited (code 0)
```

bench loops `SEND(ep_a) ; RECV(ep_b)`, mirror loops `RECV(ep_a) ; SEND(ep_b)`.
each round-trip is two synchronous rendezvous (= 4 ipc syscalls + 2 task
switches) and lands at ~14 700 mtime ticks. for the demos in earlier
versions (ping + pong, hello + echo) check `git log --oneline` and the
matching tag.

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

- ~~v0.2: real user crate with build.rs, WRITE syscall, real bytes
  flowing across the kernel boundary.~~ **shipped.**
- ~~v0.3: cooperative scheduler, two user tasks, separate page tables
  per task, context-switch in asm, SYS_YIELD wired up.~~ **shipped.**
- ~~v0.4: timer interrupt drives the same yield path so a runaway task
  can't starve the other.~~ **shipped.**
- ~~v0.5: capability table per task, synchronous send/recv on endpoint
  caps, register-passed 4-word messages, no kernel-side queueing. ping
  + pong demo.~~ **shipped.**
- ~~v0.6: fifo wait queues on endpoints (depth 8 each direction).
  ping + ping2 + pong demo: two senders parking behind one receiver,
  delivered in producer-fifo order, exit cleanup so a dead task can
  never leave a phantom waiter behind.~~ **shipped.**
- ~~v0.7: cap_dupe + cap_delete syscalls, grants through ipc (sender
  hands one cap to the receiver alongside the message words).
  end-to-end demo: ping dupes its endpoint cap, grants the dup to
  pong via the first send; pong deletes the granted cap to prove it
  arrived.~~ **shipped.**
- ~~v0.8: bench harness (user/bench, user/mirror) measuring syscall +
  ipc round-trip latency. user-readable cycle CSR. README numbers,
  ASCII architecture diagram.~~ **shipped.**
- ~~v0.9: notifications (async, 64-bit signal-set semaphore). new cap
  kind, two new syscalls, exit-cleanup integration.~~ **shipped.**
- v1.0: real device-tree parsing + virtio-blk + tiny fs, boots on
  visionfive 2.
- v0.5: hart_start SBI flow, per-hart kernel stack, big lock around the
  scheduler, then a fine-grained one.
- v0.6: device-tree parser, real memory probe, drop the `PHYS_END`
  constant. virtio-blk + tiny fs.
- v1.0: boots on a real visionfive 2.

## why call it `pith`

the part of a stem that everything else hangs off. the tiniest piece you
can take out and still have a tree. fitting for a kernel that intends to
stay small.

## license

MIT or Apache 2.0, your pick.
