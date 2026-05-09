# 07 add your own user task

every user binary in pith is its own crate under `user/`. they are not
in the kernel workspace; the kernel's [`build.rs`](../kernel/build.rs)
walks `user/<name>/` for each name in the static list, runs `cargo`
inside it, then `llvm-objcopy -O binary` on the elf. the kernel then
`include_bytes!` the flat blob.

adding a task is six small steps.

## 1. clone an existing crate

```sh
cp -r user/hello user/foo
```

## 2. point it at a fresh package name

`user/foo/Cargo.toml`:

```toml
[workspace]

[package]
name = "pith-user-foo"
version = "0.8.0"
edition = "2021"

[[bin]]
name = "foo"            # this becomes the elf file name + bin path
path = "src/main.rs"
```

the `[workspace]` line at the top is what tells cargo this crate is
NOT part of the kernel workspace. without it, cargo would refuse to
build because the parent directory has its own `[workspace]`.

## 3. write what the task does

`user/foo/src/main.rs` follows the same shape as every other user
crate: `#![no_std]`, an inline-asm `_start` that calls `main` and
falls into `SYS_EXIT`, syscall stubs in `inline asm`, and `main`
returning `!`.

the [user/hello](../user/hello/src/main.rs) crate is the smallest
real-world starting point.

## 4. tell the kernel build script to compile it

[`kernel/build.rs`](../kernel/build.rs):

```rust
for name in ["hello", "echo", "ping", "ping2", "pong", "bench", "mirror"] {
    let bin = build_user_bin(&manifest, &out_dir, name);
    println!("cargo:rustc-env=USER_{}_BIN={}", name.to_uppercase(), bin.display());
}
```

add `"foo"` to the array. the build script will produce
`USER_FOO_BIN` for the kernel to pick up.

## 5. include the binary + spawn it

[`kernel/src/main.rs`](../kernel/src/main.rs):

```rust
static USER_FOO: &[u8] = include_bytes!(env!("USER_FOO_BIN"));

// in kmain, after the existing spawns:
let foo_pid = sched::spawn("foo", USER_FOO);
```

if your task talks ipc, also install the right caps:

```rust
let ep = ipc::alloc_endpoint().expect("no free endpoints");
sched::install_cap(foo_pid,    0, cap::Cap::Endpoint(ep));
sched::install_cap(other_pid,  0, cap::Cap::Endpoint(ep));
```

## 6. cargo run

`cargo run --release` from the repo root:

- the kernel build kicks off, which fires `build.rs`, which builds
  every user crate including yours.
- on success, the merged kernel ELF lands at
  `target/riscv64gc-unknown-none-elf/release/pith`.
- the runner script in `.cargo/config.toml` hands that to qemu.

your task's stdout (via `SYS_WRITE` or `SYS_PUTC`) flows through the
uart and out to the terminal.

## what a task can and cannot do

**can**

- read `cycle` / `time` / `instret` directly (set up by `scounteren`
  in `trap::init`).
- call any of the syscalls listed in
  [docs/03-traps.md](03-traps.md) and `kernel/src/syscall.rs`.
- send + receive over endpoint capabilities its kernel-installed
  cap table grants it.
- duplicate caps locally and grant one cap per outgoing message.

**cannot**

- allocate (no heap; everything must be `'static` or stack).
- spawn other tasks (no `SYS_FORK` in v0.8). new tasks come in via
  the kernel boot path.
- read user memory of another task. ipc payload is the only sanctioned
  cross-task data path.
- touch peripherals other than the uart — there is no driver layer
  yet.

if you need any of those, file an issue or send a patch. the v1.0
roadmap covers most of them.
