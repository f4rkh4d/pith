// hello — the canonical pith user task. talks to the kernel via ecall,
// using the syscall ABI defined in kernel/src/syscall.rs. no libc, no
// runtime, no panic infra — three lines of asm and a write loop.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

// entry stub. linker puts this at the start of .text. the kernel sret's
// to USER_BASE (0x40000000) which is exactly _start. sp is set by the
// kernel before sret, so we don't touch it.
global_asm!(r#"
    .section .text.entry
    .globl _start
_start:
    call main
    # main returned: ask the kernel to shut down.
    li   a7, 0
    ecall
1:  j 1b
"#);

const SYS_EXIT:  u64 = 0;
const SYS_HI:    u64 = 1;
const SYS_PUTC:  u64 = 2;
const SYS_WRITE: u64 = 4;     // v0.2 introduces a real WRITE.

#[inline(always)]
fn ecall1(nr: u64, a0: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") nr,
            inlateout("a0") a0 => ret,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
fn ecall3(nr: u64, a0: u64, a1: u64, a2: u64) -> u64 {
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
    ecall3(SYS_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    // step 1: prove the syscall path with the cheapest call.
    ecall1(SYS_HI, 0);

    // step 2: prove WRITE works with a user-space buffer.
    write(b"u-mode echo: lord huron is on\n");

    // step 3: print one byte at a time so PUTC also gets exercised.
    for b in b"u-mode putc loop: 0123456789\n" {
        ecall1(SYS_PUTC, *b as u64);
    }

    // step 4: exit cleanly. _start handles SYS_EXIT if main ever returns
    // normally; we tail-call into it.
    let _ = ecall1(SYS_EXIT, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall1(SYS_EXIT, 1);
    loop {}
}
