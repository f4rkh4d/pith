// system call dispatch. the user task lands here after ecall via
// trap_dispatch -> EXC_ECALL_U.
//
// abi (chosen for clarity, not speed):
//   a7 = syscall number
//   a0..a5 = args
//   a0 = return value
//
// syscalls:
//   0  EXIT(code)        - shutdown via SBI; logs the exit code
//   1  HI                - print a fixed greeting (proves the ecall path)
//   2  PUTC(byte)        - write one byte to the uart
//   3  YIELD             - cooperative reschedule (single-task = noop)
//   4  WRITE(ptr, len)   - write `len` bytes at user va `ptr` to the uart
//
// reading user memory: sstatus.SUM was flipped on before sret, so the
// kernel may dereference user-mode addresses. we still bound-check the
// length to avoid an unbounded copy; v0.3 adds proper page-walk checks.

use crate::{print, println, sched, trap::TrapFrame, uart::Uart};

pub const SYS_EXIT:  u64 = 0;
pub const SYS_HI:    u64 = 1;
pub const SYS_PUTC:  u64 = 2;
pub const SYS_YIELD: u64 = 3;
pub const SYS_WRITE: u64 = 4;

/// upper bound on a single WRITE. tasks that want to print more do it in
/// chunks. cheap insurance against a wild pointer.
const WRITE_MAX: usize = 4096;

/// register name -> TrapFrame index. spec'd here so the rest of the
/// kernel doesn't carry magic numbers.
const A0: usize = 10;
const A1: usize = 11;
const A7: usize = 17;

pub fn dispatch(frame: &mut TrapFrame) {
    let nr  = frame.regs[A7];
    let a0  = frame.regs[A0];
    let a1  = frame.regs[A1];

    let ret: u64 = match nr {
        SYS_EXIT => {
            // never returns. the next task's frame replaces our stack.
            sched::exit_current(a0);
        }
        SYS_HI => {
            println!("[user]  hello from u-mode (via ecall)");
            0
        }
        SYS_PUTC => {
            Uart.putc(a0 as u8);
            0
        }
        SYS_YIELD => {
            sched::yield_now();
            0
        }
        SYS_WRITE => {
            let ptr = a0 as *const u8;
            let len = (a1 as usize).min(WRITE_MAX);
            let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
            // straight to uart; no in-kernel buffering. tasks self-rate.
            for &b in bytes {
                if b == b'\n' { Uart.putc(b'\r'); }
                Uart.putc(b);
            }
            len as u64
        }
        _ => {
            println!("[pith] unknown syscall {} (a0={:#x})", nr, a0);
            u64::MAX
        }
    };

    frame.regs[A0] = ret;
}

#[allow(dead_code)]
fn _print_dummy(s: &str) { print!("{}", s); }
