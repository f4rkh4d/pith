// system call dispatch. the user task lands here after ecall via
// trap_dispatch -> EXC_ECALL_U.
//
// abi (chosen for clarity, not speed):
//   a7 = syscall number
//   a0..a5 = args
//   a0 = return value
//
// v0.1 syscalls:
//   0  EXIT   - shutdown the machine cleanly via SBI
//   1  HI     - print a greeting (no args). proves we made it through
//                ecall, traps, and back.
//   2  PUTC   - write a single byte (a0) to the uart
//   3  YIELD  - cooperative reschedule. v0.1 is single-task, so noop.
//
// v0.2 will replace HI with WRITE(buf, len) once the user crate ships
// with a real linker script and we can pass user pointers.

use crate::{println, sbi, trap::TrapFrame, uart::Uart};

pub const SYS_EXIT:  u64 = 0;
pub const SYS_HI:    u64 = 1;
pub const SYS_PUTC:  u64 = 2;
pub const SYS_YIELD: u64 = 3;

/// register name -> TrapFrame index. spec'd here so the rest of the
/// kernel doesn't carry magic numbers.
const A0: usize = 10;
const A7: usize = 17;

pub fn dispatch(frame: &mut TrapFrame) {
    let nr  = frame.regs[A7];
    let a0  = frame.regs[A0];

    let ret: u64 = match nr {
        SYS_EXIT => {
            println!("[pith] user exited cleanly");
            sbi::shutdown();
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
            // single-task v0.1: just continue.
            0
        }
        _ => {
            println!("[pith] unknown syscall {} (a0={:#x})", nr, a0);
            u64::MAX
        }
    };

    frame.regs[A0] = ret;
}
