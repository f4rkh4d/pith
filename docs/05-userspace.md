# 05 userspace

v0.1's "user binary" is sixteen bytes hardcoded in
[`proc.rs`](../kernel/src/proc.rs):

```rust
static USER_HELLO: &[u8] = &[
    0x93, 0x08, 0x10, 0x00,  // addi a7, zero, 1     -> SYS_HI
    0x73, 0x00, 0x00, 0x00,  // ecall
    0x93, 0x08, 0x00, 0x00,  // addi a7, zero, 0     -> SYS_EXIT
    0x73, 0x00, 0x00, 0x00,  // ecall
];
```

the temptation to use `include_bytes!("user.bin")` was strong. i wanted
v0.1 to have no build-script choreography, no llvm-objcopy dependency,
nothing you can't paste into a riscv assembler and verify. so we hand-
encoded four instructions. v0.2 introduces a real `user/` workspace
with a build.rs.

## getting from kmain to u-mode

three things must be true before the cpu can sret to user space:

1. **the user pages are mapped with `U`**. `mm::PageTable::map` takes
   `perm`; we pass `PROT_RWX | (1 << 4)` for user code, `PROT_RW |
   (1 << 4)` for user stack.
2. **`sscratch` holds the kernel stack top**. on the next trap, the
   `csrrw sp, sscratch, sp` swap in `trap.S` switches us back to kernel
   memory. if `sscratch` is 0 we'd land on a null pointer and the
   double-fault would be fun.
3. **`sstatus` has `SPP=0` and `SPIE=1`**. the cpu reads both at sret
   time: SPP becomes the new privilege mode (0 = u-mode), SPIE becomes
   the new SIE (interrupts on).

the inline asm in `proc::run_first` does all three plus the jump:

```rust
asm!(
    "csrw sscratch, {kstack}",
    "csrw sepc,     {entry}",
    "csrw sstatus,  {sstatus}",
    "mv   sp,       {usp}",
    "sret",
    ...
    options(noreturn),
);
```

the `mv sp, {usp}` is the last act before we leave s-mode. if the
compiler emitted a `sd ra, 8(sp)` after that mv we would corrupt the
user's stack. `options(noreturn)` is the contract that lets us assume
no epilogue.

## what the user does

the four instructions execute as:

| step | hex          | meaning                               |
|-----:|--------------|---------------------------------------|
|   1  | `0x00100893` | `a7 = 1`                              |
|   2  | `0x00000073` | `ecall` -> kernel prints "hello"      |
|   3  | `0x00000893` | `a7 = 0`                              |
|   4  | `0x00000073` | `ecall` -> kernel calls `sbi::shutdown` |

step 2 traps: u-mode -> kernel -> dispatch -> SYS_HI -> println. then
sret returns to step 3. step 4 traps: kernel -> SYS_EXIT -> sbi
shutdown, qemu exits with status 0.

## what's missing

- a stack canary. we map four pages of user stack, the bottom one
  isn't a guard page yet.
- argv / envp. v0.2 will build a small init record at the top of the
  user stack so the user crate gets `fn main(argc, argv)`.
- ELF loader. for now `include_bytes!` of a flat blob; later, a proper
  parser that respects p_flags and p_vaddr.
