# 01 boot

opensbi hands the kernel control at physical address `0x80200000`, in
`s-mode`, with `a0` holding the hart id and `a1` pointing at the device
tree blob. that is the only contract pith depends on; everything else
we set up ourselves.

## the linker script

[`kernel/src/linker.ld`](../kernel/src/linker.ld) decides what lands at
that address. the magic three lines:

```ld
ENTRY(_start)
KERNEL_BASE = 0x80200000;
.text : ALIGN(4K) { KEEP(*(.text.entry)); *(.text .text.*); }
```

`ENTRY(_start)` tells the linker which symbol is "the entry point". the
ELF header gets that address, but opensbi ignores ELF headers when run
with `-kernel`; it just jumps to `KERNEL_BASE`. by putting `.text.entry`
first in the section list we guarantee the very first instruction at
`0x80200000` is the byte we wrote in `boot.S`.

## the boot stub

[`kernel/src/boot.S`](../kernel/src/boot.S) is six logical lines.

```asm
_start:
    bnez    a0, _park       # only hart 0 continues
    la      sp, __stack_top
    la      t0, __bss_start
    la      t1, __bss_end
1:  bgeu    t0, t1, 2f
    sd      zero, 0(t0)
    addi    t0, t0, 8
    j       1b
2:  call    kmain
```

the order matters:

1. **park other harts.** opensbi releases every hart simultaneously on
   the virt machine; on real hardware they trickle in. the kernel runs
   single-threaded for v0.1, so any non-bsp hart parks in `wfi` and
   we never look at it again.
2. **set the stack.** the linker script reserves 64 KiB after the kernel
   image and exposes the high address as `__stack_top`. we point `sp`
   at it before any C ABI call.
3. **zero bss.** the elf format says implementations may zero `.bss`;
   the qemu `-kernel` loader does not, because there is no elf loader
   in the path. we do it ourselves.
4. **fall through to rust.** `call kmain` preserves `a0` and `a1`, so
   `kmain(hart, dtb)` receives the same registers opensbi passed us.

## what kmain does first

[`kernel/src/main.rs`](../kernel/src/main.rs) just wires the modules in
order. uart first, so panics that fire during later setup still print.
then memory, then traps, then process, then sret.

```rust
uart::init();
mm::init();
trap::init();
proc::init();
proc::run_first();
```

if you change anything before `uart::init()` runs and it panics, the
machine will spin silently. the only way to debug that case is the
qemu monitor (`ctrl-a c` then `info registers`). been there.

## what we did not do

- read the device tree. opensbi's device tree contains the actual
  memory map, the uart base, the PLIC base, the timebase frequency.
  v0.1 hardcodes the qemu-virt values. v0.5 will parse the dtb
  for real.
- enable the FPU. neither the kernel nor the user binary needs it.
  if you write rust code that emits `f*` instructions the trap will
  fire on first use and the kernel will print "unhandled exception
  cause 2" (illegal instruction). just disable f-extension or
  context-switch its state, your call.
