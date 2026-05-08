# 02 uart

the qemu-virt machine puts a 16550-compatible uart at physical address
`0x10000000`. opensbi already initialised it for its own boot log; we
re-initialise from scratch so the kernel does not depend on whatever
state firmware left behind.

## registers

we touch four registers, each one byte at a one-byte offset from base:

| offset | name | role                              |
|-------:|------|-----------------------------------|
| `+0`   | THR  | transmit holding (write)          |
| `+0`   | RBR  | receive buffer (read)             |
| `+1`   | IER  | interrupt enable                  |
| `+2`   | FCR  | fifo control                      |
| `+3`   | LCR  | line control (8N1, baud divisor)  |
| `+5`   | LSR  | line status (THRE, DR, etc.)      |

[`kernel/src/uart.rs`](../kernel/src/uart.rs) wraps the four into
methods + a `core::fmt::Write` adapter. `init()`:

```rust
u.write(LCR, 0b0000_0011);  // 8 data bits, 1 stop bit, no parity
u.write(FCR, 0b0000_0111);  // enable + clear both fifos
u.write(IER, 0);            // mask all uart interrupts (we poll)
```

we do not configure baud. qemu doesn't care. real hardware does — that
goes on the v0.5 list with the device tree work.

## putc + getc

```rust
pub fn putc(&self, c: u8) {
    while self.read(LSR) & LSR_THRE == 0 {}  // wait for empty THR
    self.write(THR, c);
}
```

a busy loop is fine for kernel printing because no real workload runs
during a print. once user tasks land we'll re-route writes through the
WRITE syscall and keep the busy-wait inside the kernel.

`getc()` returns `Option<u8>` so the caller can poll: a future shell
will spin on this until DR is set. v0.1 doesn't read input.

## the println! macro

paranoid choice: keep the macro in the same module that owns the uart,
not a global "logger" abstraction. one less place where a bug can hide
between my keystrokes and qemu stdout.

```rust
print!("[pith] paging on (sv39)");  // no \n
println!("[pith] paging on (sv39)"); // \n -> \r\n on the wire
```

we translate `\n` to `\r\n` on output so terminal emulators don't print
staircases. classic serial gotcha.
