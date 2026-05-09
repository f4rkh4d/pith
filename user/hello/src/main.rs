// hello — first user task. iterates with yields so the cooperative
// scheduler can interleave with the second task.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

global_asm!(r#"
    .section .text.entry
    .globl _start
_start:
    call main
    li   a7, 0
    ecall
1:  j 1b
"#);

const SYS_EXIT:  u64 = 0;
const SYS_HI:    u64 = 1;
const SYS_YIELD: u64 = 3;
const SYS_WRITE: u64 = 4;

#[inline(always)]
fn ecall(nr: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") nr,
            inlateout("a0") a0 => ret,
            in("a1") a1,
            in("a2") a2,
            options(nostack),
        );
    }
    ret
}

fn write(s: &[u8]) {
    ecall(SYS_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}

#[allow(dead_code)]
fn yield_now() {
    ecall(SYS_YIELD, 0, 0, 0);
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    ecall(SYS_HI, 0, 0, 0);
    // no yields. with v0.4's timer interrupt, the scheduler preempts
    // us inside this loop so echo gets a turn anyway.
    for i in 0u64..30 {
        let mut buf = *b"hello tick XX\n";
        buf[11] = b'0' + ((i / 10) as u8);
        buf[12] = b'0' + ((i % 10) as u8);
        write(&buf);
        // a small busy-wait so the loop body takes long enough for the
        // 10 ms timer quantum to bite at least once before we exit.
        for _ in 0..3_000_000 { unsafe { asm!("nop"); } }
    }
    write(b"hello done\n");
    let _ = ecall(SYS_EXIT, 0, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall(SYS_EXIT, 1, 0, 0);
    loop {}
}
