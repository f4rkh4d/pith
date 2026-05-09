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

use crate::sched;

pub const MAX_ENDPOINTS: usize = 8;

pub type EndpointId = u8;

#[derive(Clone, Copy)]
pub struct Message {
    pub label: u64,
    pub words: [u64; 3],
}

#[derive(Clone, Copy)]
pub struct Endpoint {
    pub waiting_sender:   Option<u32>,    // pid (1..=MAX_PROCS)
    pub waiting_receiver: Option<u32>,
    pub stash:            Option<Message>, // sender's outbound msg while blocked
}

impl Endpoint {
    pub const fn empty() -> Self {
        Self { waiting_sender: None, waiting_receiver: None, stash: None }
    }
}

static mut ENDPOINTS: [Endpoint; MAX_ENDPOINTS] = [Endpoint::empty(); MAX_ENDPOINTS];

pub fn alloc_endpoint() -> Option<EndpointId> {
    unsafe {
        for (i, ep) in ENDPOINTS.iter_mut().enumerate() {
            if ep.waiting_sender.is_none() && ep.waiting_receiver.is_none() && ep.stash.is_none() {
                // mark "in use" by leaving zeros; the test above is a free check.
                // we just hand out the slot. caller must remember they own it.
                let _ = ep;
                return Some(i as EndpointId);
            }
        }
        None
    }
}

#[derive(Clone, Copy, Debug)]
pub enum IpcError {
    BadEndpoint,
    Busy,           // someone else already waiting in this direction
}

/// rendezvous-style send. on success the sender returns immediately
/// with status 0. on no-receiver-waiting, the sender blocks until a
/// receiver shows up (return value will be filled in then).
pub fn send(ep_id: EndpointId, msg: Message) -> Result<SendOutcome, IpcError> {
    if ep_id as usize >= MAX_ENDPOINTS { return Err(IpcError::BadEndpoint); }
    unsafe {
        let ep = &mut ENDPOINTS[ep_id as usize];
        if let Some(rcv_pid) = ep.waiting_receiver.take() {
            // fast path: deliver into the receiver's frame and wake it.
            sched::deliver_to(rcv_pid, msg);
            return Ok(SendOutcome::Delivered);
        }
        if ep.waiting_sender.is_some() {
            return Err(IpcError::Busy);
        }
        // slow path: stash the message on the sender's pending slot
        // (kept on the endpoint to keep send-state local). when a
        // receiver shows up, recv() consumes it, wakes us with status 0.
        ep.waiting_sender = Some(sched::current_pid());
        ep.stash = Some(msg);
        sched::block_on_send(ep_id);
        // when control returns here we've already been resumed and our
        // a0 has been set in the trap frame. report "blocked then
        // delivered" so the syscall site doesn't overwrite a0.
        Ok(SendOutcome::DeliveredAfterBlock)
    }
}

/// rendezvous-style recv. on a waiting sender, deliver immediately.
/// otherwise block until one arrives.
pub fn recv(ep_id: EndpointId) -> Result<RecvOutcome, IpcError> {
    if ep_id as usize >= MAX_ENDPOINTS { return Err(IpcError::BadEndpoint); }
    unsafe {
        let ep = &mut ENDPOINTS[ep_id as usize];
        if let Some(snd_pid) = ep.waiting_sender.take() {
            let msg = ep.stash.take().expect("sender blocked without stashing");
            // wake the sender with status 0 (delivered).
            sched::wake_with_status(snd_pid, 0);
            return Ok(RecvOutcome::Got(msg));
        }
        if ep.waiting_receiver.is_some() {
            return Err(IpcError::Busy);
        }
        ep.waiting_receiver = Some(sched::current_pid());
        sched::block_on_recv(ep_id);
        // by the time we get here, sched::deliver_to has already poked
        // our trap frame regs with the message and set status 0.
        Ok(RecvOutcome::Delivered)
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
