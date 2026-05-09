// pong — receives 5 ipc messages on the endpoint cap and prints each
// one's label + first word.

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
const SYS_WRITE: u64 = 4;
const SYS_RECV:  u64 = 6;

const EP_CAP: u64 = 0;

#[inline(always)]
fn ecall_n(nr: u64, a0: u64, a1: u64, a2: u64) -> u64 {
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

/// recv into 4 registers. returns (label, w0, w1, w2).
#[inline(always)]
fn ipc_recv(cap: u64) -> (u64, u64, u64, u64) {
    let mut a0: u64;
    let mut a1: u64;
    let mut a2: u64;
    let mut a3: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") SYS_RECV,
            inlateout("a0") cap => a0,
            lateout("a1") a1,
            lateout("a2") a2,
            lateout("a3") a3,
            options(nostack),
        );
    }
    (a0, a1, a2, a3)
}

fn write(s: &[u8]) {
    ecall_n(SYS_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}

fn print_hex16(label: u64) {
    let mut buf = [b'0'; 4];
    for i in 0..4 {
        let nib = ((label >> ((3 - i) * 4)) & 0xf) as u8;
        buf[i] = if nib < 10 { b'0' + nib } else { b'a' + nib - 10 };
    }
    write(&buf);
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    write(b"pong  starting\n");
    for _ in 0u64..5 {
        let (label, w0, _w1, _w2) = ipc_recv(EP_CAP);
        write(b"pong  got 0x");
        print_hex16(label & 0xffff);
        let mut tail = *b" w0=X\n";
        tail[4] = b'0' + (w0 as u8 & 0xf);
        write(&tail);
    }
    write(b"pong  done\n");
    let _ = ecall_n(SYS_EXIT, 0, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall_n(SYS_EXIT, 1, 0, 0);
    loop {}
}
