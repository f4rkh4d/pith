// bench — measures pith's hot paths.
//   1. empty-syscall latency (SYS_HI in a tight loop, but without
//      kernel printing — we use a noop syscall: SYS_YIELD with a
//      mirror task that's not ready yet falls into "no other task"
//      and returns straight away. so we just measure SYS_PUTC of \0
//      which is trivially short).
//   2. ipc round-trip: send on ep_a, recv on ep_b. mirror task
//      runs the inverse.
//
// reads `cycle` CSR via inline asm for cycle counts; qemu virt clocks
// it at 1 cycle per instruction (roughly), so the numbers are
// representative of an actual rv64 cpu.

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

const SYS_EXIT:          u64 = 0;
const SYS_PUTC:          u64 = 2;
const SYS_YIELD:         u64 = 3;
const SYS_WRITE:         u64 = 4;
const SYS_SEND:          u64 = 5;
const SYS_RECV:          u64 = 6;
const SYS_NOTIFY_SIGNAL: u64 = 9;
const SYS_NOTIFY_WAIT:   u64 = 10;

const NOTIF_CAP: u64 = 2;     // installed by kmain

const EP_A: u64 = 0;        // bench sends here, mirror recvs.
const EP_B: u64 = 1;        // mirror sends here, bench recvs.
const NO_GRANT: u64 = 0xff;

const N_SYSCALL: u64 = 10_000;
const N_IPC:     u64 = 5_000;

#[inline(always)]
fn rd_cycle() -> u64 {
    let c: u64;
    unsafe { asm!("rdcycle {0}", out(reg) c, options(nostack, nomem)); }
    c
}

#[inline(always)]
fn ecall0(nr: u64) {
    unsafe {
        asm!("ecall",
            in("a7") nr,
            lateout("a0") _,
            options(nostack));
    }
}

#[inline(always)]
fn ecall1(nr: u64, a0: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!("ecall",
            in("a7") nr,
            inlateout("a0") a0 => ret,
            options(nostack));
    }
    ret
}

#[inline(always)]
fn ecall_w(nr: u64, a0: u64, a1: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!("ecall",
            in("a7") nr,
            inlateout("a0") a0 => ret,
            in("a1") a1,
            options(nostack));
    }
    ret
}

#[inline(always)]
fn ipc_send(cap: u64, label: u64) -> u64 {
    let mut ret: u64;
    unsafe {
        asm!("ecall",
            in("a7") SYS_SEND,
            inlateout("a0") cap => ret,
            in("a1") label,
            in("a2") 0u64, in("a3") 0u64, in("a4") 0u64,
            in("a5") NO_GRANT,
            options(nostack));
    }
    ret
}

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

fn write(s: &[u8]) {
    ecall_w(SYS_WRITE, s.as_ptr() as u64, s.len() as u64);
}

/// print a u64 in decimal.
fn print_u64(mut n: u64) {
    if n == 0 { ecall1(SYS_PUTC, b'0' as u64); return; }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 { buf[i] = (n % 10) as u8 + b'0'; n /= 10; i += 1; }
    while i > 0 { i -= 1; ecall1(SYS_PUTC, buf[i] as u64); }
}

fn report(label: &[u8], total: u64, n: u64) {
    write(label);
    write(b": total=");
    print_u64(total);
    write(b" cycles, n=");
    print_u64(n);
    write(b", avg=");
    print_u64(total / n);
    write(b" cyc/op\n");
}

#[no_mangle]
pub extern "C" fn main() -> ! {
    write(b"bench  starting (qemu-virt, rv64, 10 MHz time base)\n");

    // ---- 1. empty syscall: SYS_YIELD with mirror blocked (no other
    //         runnable task, so yield_now returns immediately) ----
    let t0 = rd_cycle();
    for _ in 0..N_SYSCALL { ecall0(SYS_YIELD); }
    let t1 = rd_cycle();
    report(b"bench  syscall (YIELD)", t1 - t0, N_SYSCALL);

    // ---- 2. IPC round-trip ----
    // synchronization with mirror: mirror is already in its loop, both
    // sides recv-then-send. bench is send-then-recv. one round-trip =
    // 2 ipc ops on each side. we measure full round-trips.
    let t0 = rd_cycle();
    for i in 0..N_IPC {
        ipc_send(EP_A, i);
        let _r = ipc_recv(EP_B);
    }
    let t1 = rd_cycle();
    report(b"bench  ipc round-trip ", t1 - t0, N_IPC);

    // ---- 3. notification self-test: signal three bits, wait, verify ----
    // since the same task signals + waits, the wait sees the bits we
    // just set and returns immediately (no blocking; the fast path
    // through ipc::wait).
    let _ = ecall_w(SYS_NOTIFY_SIGNAL, NOTIF_CAP, 0b1011);
    let mut a0: u64;
    unsafe {
        asm!("ecall",
            in("a7") SYS_NOTIFY_WAIT,
            inlateout("a0") NOTIF_CAP => a0,
            options(nostack));
    }
    if a0 == 0b1011 {
        write(b"bench  notif: signaled + waited 0b1011 ok\n");
    } else {
        write(b"bench  notif: MISMATCH\n");
    }

    write(b"bench  done\n");
    let _ = ecall1(SYS_EXIT, 0);
    loop {}
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    let _ = ecall1(SYS_EXIT, 1);
    loop {}
}
