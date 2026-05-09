// mirror — bench's ipc partner. recv on ep_a, echo to ep_b. exits
// after N round-trips so the system shuts down cleanly.

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
const SYS_SEND:  u64 = 5;
const SYS_RECV:  u64 = 6;
const NO_GRANT:  u64 = 0xff;

const EP_A: u64 = 0;
const EP_B: u64 = 1;
const N_IPC: u64 = 5_000;

#[inline(always)]
fn ipc_recv(cap: u64) -> u64 {
    let mut a0: u64;
    unsafe {
        asm!("ecall",
            in("a7") SYS_RECV,
            inlateout("a0") cap => a0,
            in("a1") NO_GRANT,
            lateout("a2") _, lateout("a3") _,
            options(nostack));
    }
    a0
}

#[inline(always)]
fn ipc_send(cap: u64, label: u64) {
    unsafe {
        asm!("ecall",
            in("a7") SYS_SEND,
            in("a0") cap,
            in("a1") label,
            in("a2") 0u64, in("a3") 0u64, in("a4") 0u64,
            in("a5") NO_GRANT,
            lateout("a0") _,
            options(nostack));
    }
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    for _ in 0..N_IPC {
        let l = ipc_recv(EP_A);
        ipc_send(EP_B, l);
    }
    unsafe { asm!("ecall", in("a7") SYS_EXIT, in("a0") 0u64, options(nostack)); }
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    unsafe { asm!("ecall", in("a7") SYS_EXIT, in("a0") 1u64, options(nostack)); }
    loop {}
}
