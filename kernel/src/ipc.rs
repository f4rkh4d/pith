// inter-process communication. v0.5 implements synchronous send/recv
// against a small endpoint pool. message payload is 32 bytes (one
// label + three data words), passed through the trap frame's a0..a3
// without ever touching user memory in the kernel.
//
// the design is one notch simpler than seL4 to keep the impl honest:
//   - one waiting sender + one waiting receiver per endpoint (no FIFO yet)
//   - rendezvous-only (no kernel-side queueing of full messages)
//   - capabilities are integer handles into the per-task CapTable
//
// queue depth grows in v0.6 alongside capability derivation.

use crate::{cap, sched};

pub const MAX_ENDPOINTS: usize = 8;
pub const QUEUE_DEPTH:   usize = 8;

pub type EndpointId = u8;

#[derive(Clone, Copy, Default)]
pub struct Message {
    pub label: u64,
    pub words: [u64; 3],
}

/// a parked sender carries its outgoing message with it; otherwise
/// senders racing for the same endpoint would clobber the stash slot
/// they shared in v0.5. v0.7 also lets a sender attach one capability
/// to a message — granted to the receiver on rendezvous.
#[derive(Clone, Copy, Default)]
pub struct WaitingSender {
    pub pid: u32,
    pub msg: Message,
    pub grant: Option<cap::Cap>,
}

/// fifo waitlist. front = items[0]; we shift on pop. small N, no heap,
/// fine.
#[derive(Clone, Copy)]
pub struct WaitQ<T: Copy + Default> {
    pub items: [T; QUEUE_DEPTH],
    pub len:   usize,
}

impl<T: Copy + Default> WaitQ<T> {
    pub const fn new() -> Self {
        Self { items: [const { unsafe { core::mem::zeroed() } }; QUEUE_DEPTH], len: 0 }
    }
    pub fn is_full(&self)  -> bool { self.len == QUEUE_DEPTH }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    pub fn push(&mut self, item: T) -> Result<(), ()> {
        if self.is_full() { return Err(()); }
        self.items[self.len] = item;
        self.len += 1;
        Ok(())
    }
    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() { return None; }
        let head = self.items[0];
        for i in 1..self.len {
            self.items[i - 1] = self.items[i];
        }
        self.len -= 1;
        Some(head)
    }
    /// remove pid from a queue if present (for cleanup on exit).
    pub fn remove_pid<F: Fn(&T) -> u32>(&mut self, pid: u32, get_pid: F) {
        let mut w = 0;
        for r in 0..self.len {
            if get_pid(&self.items[r]) != pid {
                self.items[w] = self.items[r];
                w += 1;
            }
        }
        self.len = w;
    }
}

#[derive(Clone, Copy)]
pub struct Endpoint {
    pub senders: WaitQ<WaitingSender>,
    pub recvers: WaitQ<u32>,
    /// false until alloc_endpoint hands out this slot.
    pub allocated: bool,
}

impl Endpoint {
    pub const fn empty() -> Self {
        Self {
            senders:   WaitQ::new(),
            recvers:   WaitQ::new(),
            allocated: false,
        }
    }
}

static mut ENDPOINTS: [Endpoint; MAX_ENDPOINTS] = [Endpoint::empty(); MAX_ENDPOINTS];

pub fn alloc_endpoint() -> Option<EndpointId> {
    unsafe {
        for (i, ep) in ENDPOINTS.iter_mut().enumerate() {
            if !ep.allocated {
                ep.allocated = true;
                return Some(i as EndpointId);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub enum IpcError {
    BadEndpoint,
    QueueFull,    // ipc partner queue saturated
}

/// rendezvous-style send. optionally grants one capability to the
/// receiver alongside the message words.
pub fn send(
    ep_id: EndpointId,
    msg: Message,
    grant: Option<cap::Cap>,
) -> Result<SendOutcome, IpcError> {
    if ep_id as usize >= MAX_ENDPOINTS { return Err(IpcError::BadEndpoint); }
    unsafe {
        let ep = &mut ENDPOINTS[ep_id as usize];
        if !ep.allocated { return Err(IpcError::BadEndpoint); }

        if let Some(rcv_pid) = ep.recvers.pop_front() {
            sched::deliver_to(rcv_pid, msg, grant);
            return Ok(SendOutcome::Delivered);
        }
        ep.senders.push(WaitingSender { pid: sched::current_pid(), msg, grant })
            .map_err(|_| IpcError::QueueFull)?;
        sched::block_on_send(ep_id);
        Ok(SendOutcome::DeliveredAfterBlock)
    }
}

/// rendezvous-style recv. on a waiting sender (fifo), pop and deliver.
/// `grant_dst` is the receiver's slot for any granted cap; if the
/// message carries no grant, this slot is left untouched.
pub fn recv(ep_id: EndpointId, grant_dst: u8) -> Result<RecvOutcome, IpcError> {
    if ep_id as usize >= MAX_ENDPOINTS { return Err(IpcError::BadEndpoint); }
    unsafe {
        let ep = &mut ENDPOINTS[ep_id as usize];
        if !ep.allocated { return Err(IpcError::BadEndpoint); }

        if let Some(s) = ep.senders.pop_front() {
            // install the granted cap on the receiver (= us, the
            // current task) before we return up to syscall::dispatch.
            if let Some(g) = s.grant {
                sched::current_caps_mut()
                    .install(grant_dst as usize, g).ok();
            }
            sched::wake_with_status(s.pid, 0);
            return Ok(RecvOutcome::Got(s.msg));
        }
        // record where to install a grant when one arrives. we keep it
        // alongside the receiver in the waitq via a parallel store on
        // the Process. simplest: stash on the current proc's saved
        // frame slot (a4); deliver_to reads it back on rendezvous.
        sched::set_current_grant_dst(grant_dst);
        ep.recvers.push(sched::current_pid())
            .map_err(|_| IpcError::QueueFull)?;
        sched::block_on_recv(ep_id);
        Ok(RecvOutcome::Delivered)
    }
}

/// remove a pid from every endpoint's wait queues. called by sched on
/// task exit so a dead task can never leave a phantom waiter behind.
pub fn drop_waiters(pid: u32) {
    unsafe {
        for ep in ENDPOINTS.iter_mut() {
            if !ep.allocated { continue; }
            ep.senders.remove_pid(pid, |s| s.pid);
            ep.recvers.remove_pid(pid, |&p| p);
        }
    }
}

pub enum SendOutcome {
    Delivered,             // syscall handler returns 0 in a0
    DeliveredAfterBlock,   // a0 already filled by waker
}

pub enum RecvOutcome {
    Got(Message),
    Delivered,             // a0..a3 already filled by waker
}
