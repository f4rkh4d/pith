// system call dispatch. trap_dispatch routes EXC_ECALL_U here.
//
// abi:
//   a7 = syscall number
//   a0..a5 = args (a0 also carries the return value)
//
// syscalls (v0.5):
//   0  EXIT(code)             - shutdown via SBI; logs the exit code
//   1  HI                     - print a fixed greeting
//   2  PUTC(byte)             - write one byte to the uart
//   3  YIELD                  - cooperative reschedule
//   4  WRITE(ptr, len)        - write `len` bytes at user va `ptr` to the uart
//   5  SEND(cap, l, w0,w1,w2) - synchronous ipc send. blocks if no receiver
//   6  RECV(cap)              - synchronous ipc recv. blocks until a sender
//                               arrives. returns label in a0, words in a1..a3
//
// each match arm decides for itself whether to write a0; SEND/RECV short-
// circuit because the wake path has already filled in the registers.

use crate::{
    cap::Cap, ipc, println, sched, trap::TrapFrame, uart::Uart,
};

pub const SYS_EXIT:  u64 = 0;
pub const SYS_HI:    u64 = 1;
pub const SYS_PUTC:  u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const SYS_WRITE: u64 = 4;
pub const SYS_SEND:  u64 = 5;
pub const SYS_RECV:  u64 = 6;

const WRITE_MAX: usize = 4096;

const A0: usize = 10;
const A1: usize = 11;
const A2: usize = 12;
const A3: usize = 13;
const A4: usize = 14;
const A7: usize = 17;

pub fn dispatch(frame: &mut TrapFrame) {
    let nr = frame.regs[A7];
    let a0 = frame.regs[A0];
    let a1 = frame.regs[A1];
    let a2 = frame.regs[A2];
    let a3 = frame.regs[A3];
    let a4 = frame.regs[A4];

    match nr {
        SYS_EXIT => {
            sched::exit_current(a0);
        }
        SYS_HI => {
            println!("[user]  hello from u-mode (via ecall)");
            frame.regs[A0] = 0;
        }
        SYS_PUTC => {
            Uart.putc(a0 as u8);
            frame.regs[A0] = 0;
        }
        SYS_YIELD => {
            sched::yield_now();
            frame.regs[A0] = 0;
        }
        SYS_WRITE => {
            let ptr = a0 as *const u8;
            let len = (a1 as usize).min(WRITE_MAX);
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
            for &b in bytes {
                if b == b'\n' { Uart.putc(b'\r'); }
                Uart.putc(b);
            }
            frame.regs[A0] = len as u64;
        }
        SYS_SEND => {
            let ep = match resolve_endpoint(a0 as usize) {
                Some(e) => e,
                None    => { frame.regs[A0] = u64::MAX; return; }
            };
            let msg = ipc::Message { label: a1, words: [a2, a3, a4] };
            match ipc::send(ep, msg) {
                Ok(ipc::SendOutcome::Delivered) => frame.regs[A0] = 0,
                // DeliveredAfterBlock: a0 was set by the waker; do nothing.
                Ok(ipc::SendOutcome::DeliveredAfterBlock) => {}
                Err(_) => frame.regs[A0] = u64::MAX,
            }
        }
        SYS_RECV => {
            let ep = match resolve_endpoint(a0 as usize) {
                Some(e) => e,
                None    => { frame.regs[A0] = u64::MAX; return; }
            };
            match ipc::recv(ep) {
                Ok(ipc::RecvOutcome::Got(m)) => {
                    frame.regs[A0] = m.label;
                    frame.regs[A1] = m.words[0];
                    frame.regs[A2] = m.words[1];
                    frame.regs[A3] = m.words[2];
                }
                // Delivered: a0..a3 were set by the waker; do nothing.
                Ok(ipc::RecvOutcome::Delivered) => {}
                Err(_) => frame.regs[A0] = u64::MAX,
            }
        }
        _ => {
            println!("[pith] unknown syscall {} (a0={:#x})", nr, a0);
            frame.regs[A0] = u64::MAX;
        }
    }
}

/// resolve the current task's cap handle to an endpoint id.
fn resolve_endpoint(handle: usize) -> Option<ipc::EndpointId> {
    let caps = sched::current_caps_mut();
    match caps.get(handle) {
        Some(Cap::Endpoint(id)) => Some(*id),
        _ => None,
    }
}
