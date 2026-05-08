# 03 traps

a trap is anything that yanks the cpu out of the current stream and
into a fixed handler: an `ecall`, a page fault, an interrupt, an
illegal instruction. on riscv-s the handler address comes from `stvec`
and the cause comes from `scause`.

## one vector

[`trap.S`](../kernel/src/trap.S) is the single entry. we use direct
mode (`stvec` low 2 bits = 0), so every trap lands at the same address
and we dispatch in software. vectored mode is faster but more code,
not worth it before we have a scheduler.

## sscratch tells us where we came from

```asm
trap_entry:
    csrrw sp, sscratch, sp     # swap. sscratch <-> sp.
    bnez  sp, _from_user
_from_kernel: ...
_from_user:   ...
```

contract:

- when running **u-mode**, `sscratch` holds the kernel stack top.
- when running **s-mode**, `sscratch` holds 0.

the swap therefore does the right thing in both cases:

- u-mode trap: after swap, `sp` = kernel stack top, `sscratch` =
  saved user sp. we save the frame on the kernel stack and run.
- s-mode trap: after swap, `sp` = 0, `sscratch` = saved kernel sp.
  the `bnez sp, _from_user` falls through, we swap back, and pull
  frame space out of the kernel sp we already had.

## TrapFrame

```rust
#[repr(C)]
pub struct TrapFrame {
    pub regs: [u64; 32],   // x0..x31 (x0 ignored, kept for indexing)
    pub sepc: u64,
    pub sstatus: u64,
    pub _pad: u64,         // 16-byte alignment
}
```

280 bytes. the assembly stores into fixed offsets that match the rust
layout. if you reorder the rust fields, edit the asm too — there is no
compile-time check.

## dispatch

```rust
fn trap_dispatch(frame: &mut TrapFrame) {
    let scause = csrr!(scause);
    if scause & SCAUSE_INTERRUPT != 0 { ...interrupt... }
    else                              { ...exception... }
}
```

cause bit 63 separates interrupt from exception. inside each, the low
bits select the kind. v0.1 handles three cases:

- `EXC_ECALL_U` = 8 → bump `sepc` by 4 and call `syscall::dispatch`.
- `EXC_PAGE_FAULT_*` = 12/13/15 → log + shutdown. real handling later.
- timer interrupt = 5 → re-arm via SBI, ignore.

everything else: log the cause and shutdown. better to crash loud than
to spin into a wedge.

## why we don't bump sepc on interrupts

the spec is annoying here: for **exceptions**, `sepc` points at the
faulting instruction, so the handler advances it before sret to skip
past the trapping op. for **interrupts**, `sepc` points at the next
instruction to run, so advancing it would skip a real instruction.
the dispatch code only adds 4 in the ecall arm.
