// ping — sends 5 ipc messages over the endpoint cap installed by
// kmain at slot 0, then exits.

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

const SYS_EXIT:       u64 = 0;
const SYS_WRITE:      u64 = 4;
const SYS_SEND:       u64 = 5;
const SYS_CAP_DUPE:   u64 = 7;
const SYS_CAP_DELETE: u64 = 8;

const EP_CAP: u64 = 0;       // installed by kmain into slot 0.
const NO_GRANT: u64 = 0xff;

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

#[inline(always)]
fn ipc_send(cap: u64, label: u64, w0: u64, w1: u64, w2: u64, grant_src: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") SYS_SEND,
            inlateout("a0") cap => ret,
            in("a1") label,
            in("a2") w0,
            in("a3") w1,
            in("a4") w2,
            in("a5") grant_src,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
fn cap_dupe(src: u64, dst: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") SYS_CAP_DUPE,
            inlateout("a0") src => ret,
            in("a1") dst,
            options(nostack),
        );
    }
    ret
}

#[inline(always)]
fn cap_delete(slot: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!(
            "ecall",
            in("a7") SYS_CAP_DELETE,
            inlateout("a0") slot => ret,
            options(nostack),
        );
    }
    ret
}

fn write(s: &[u8]) {
    ecall_n(SYS_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    write(b"ping  starting\n");

    // exercise the cap-table syscalls. dupe slot 0 -> 1, delete 1, dupe again.
    let _ = cap_dupe(EP_CAP, 1);
    let _ = cap_delete(1);
    let _ = cap_dupe(EP_CAP, 1);
    write(b"ping  caps: dupe + delete ok\n");

    for i in 0u64..5 {
        let label = 0xAA00 + i;
        // on the very first send, grant the duplicated cap (slot 1)
        // to pong's slot 7. subsequent sends carry no grant.
        let grant_src = if i == 0 { 1 } else { NO_GRANT };
        let r = ipc_send(EP_CAP, label, i, i * 10, i * 100, grant_src);
        let mut buf = *b"ping  sent X (s=Y)\n";
        buf[11] = b'0' + i as u8;
        buf[16] = if r == 0 { b'k' } else { b'!' };
        write(&buf);
    }
    write(b"ping  done\n");
    let _ = ecall_n(SYS_EXIT, 0, 0, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall_n(SYS_EXIT, 1, 0, 0);
    loop {}
}
