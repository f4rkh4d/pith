// inter-process communication. v0.1 ships only the data structures and
// a doc-test demo of the planned API; the real send/recv path lights up
// in v0.2 once the scheduler can block & wake tasks.
//
// design: synchronous, copy-once, capability-gated. a sender finds a
// receiver via a small integer endpoint id (later: capability handle),
// hands over a fixed 64-byte message, and the kernel copies it across.
// asynchronous variants and bigger payloads come behind notifications,
// not by stretching this primitive.

#![allow(dead_code)]

pub const MSG_BYTES: usize = 64;

/// 64-byte message body. layout is intentionally fixed: the ABI does
/// not grow with the kernel.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Message {
    pub label: u64,
    pub words: [u64; 7],
}

/// endpoint state. an endpoint is a meeting place: at most one sender
/// and one receiver are blocked on it at any time. seL4 calls these
/// "endpoints"; we keep the term.
#[derive(Default)]
pub struct Endpoint {
    pub waiting_sender:   Option<TaskId>,
    pub waiting_receiver: Option<TaskId>,
}

pub type TaskId = u32;

// future:
//   pub fn send(ep: EndpointId, m: &Message) -> Result<(), IpcError>;
//   pub fn recv(ep: EndpointId, m: &mut Message) -> Result<TaskId, IpcError>;
//   pub fn call(ep: EndpointId, m: &mut Message) -> Result<(), IpcError>;
