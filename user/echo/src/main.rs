// echo — second user task. counts to 5 and yields between each
// iteration so the cooperative scheduler can hand the cpu to hello,
// and back, and the boot log shows them interleaved.

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
    // also no yields. the timer is the only thing that keeps the
    // hello task alive while we're inside this loop.
    for i in 0u64..30 {
        let mut buf = *b"echo  tick XX\n";
        buf[11] = b'0' + ((i / 10) as u8);
        buf[12] = b'0' + ((i % 10) as u8);
        write(&buf);
        for _ in 0..3_000_000 { unsafe { asm!("nop"); } }
    }
    write(b"echo  done\n");
    let _ = ecall(SYS_EXIT, 0, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall(SYS_EXIT, 1, 0, 0);
    loop {}
}
