# 06 ipc

inter-process communication is the part where seL4 lives or dies. the
kernel's only job, once you cut everything else away, is to be the
trusted broker between mutually-distrusting tasks. v0.1 ships the data
types and the design notes; v0.2 wires the fast path.

## design

- **synchronous.** sender and receiver must both be ready before the
  copy happens. no kernel-side queues, no buffering. blocking is the
  primitive that makes scheduling honest.
- **fixed-size messages.** 64 bytes per send. larger payloads go behind
  a separate **shared-memory** mechanism (`map`/`unmap` syscalls in
  v0.4); they don't stretch the ipc primitive.
- **capability-gated.** in v0.3 a process talks to an endpoint via a
  capability handle, not an integer id. you can't forge an endpoint
  the way you can forge a pid.

```rust
pub const MSG_BYTES: usize = 64;

#[repr(C)]
pub struct Message {
    pub label: u64,        // user-defined "what kind of message"
    pub words: [u64; 7],   // payload
}

pub struct Endpoint {
    pub waiting_sender:   Option<TaskId>,
    pub waiting_receiver: Option<TaskId>,
}
```

## why no notifications yet

seL4 has a second primitive — **notifications** — that are async,
broadcast, set-bits semaphores. they're how you wake an interrupt
handler or signal multiple readers. they belong in v0.3 alongside
capabilities; until the scheduler can put a task to sleep it's
moot.

## fast path notes (for v0.2)

three pieces matter for ipc latency:

1. **register-only payloads**. when the message fits in registers
   (`label` + a couple words), don't touch memory. seL4 makes this
   the common case and the latency drops to ~250 cycles on x86.
2. **direct switch**. the moment send completes, jump straight to
   the receiver — don't return through the scheduler. this skips a
   priority queue insertion and a context switch round-trip.
3. **lock-free queues for endpoint state**. the v0.4 fine-grained
   scheduler will need this; v0.2 can use a big kernel lock.

## what to read next

- "lightweight remote procedure call", bershad et al. (1990). the
  paper that named the trick of skipping the scheduler.
- seL4 manual chapter 4. the API the v0.3 kernel will copy.
- the [original sel4 evaluation](https://sel4.systems/About/seL4-whitepaper.pdf).
  pay attention to the ipc-latency tables; they're the bar.
