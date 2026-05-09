// pith kernel. boots from opensbi at 0x80200000, sets up paging + traps,
// spawns user tasks, wires their initial capabilities, and hands off to
// the scheduler.

#![no_std]
#![no_main]
// these are intentional: small kernel, every static mut is reached
// from a single hart at a time, plenty of "future hooks" sit unused
// while the rest of the kernel grows around them.
#![allow(
    clippy::missing_safety_doc,
    dead_code,
    static_mut_refs,
    function_casts_as_integer,
)]

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("trap.S"));
global_asm!(include_str!("sched.S"));

mod uart;
mod trap;
mod mm;
mod sched;
mod ipc;
mod cap;
mod syscall;
mod sbi;

static USER_BENCH:  &[u8] = include_bytes!(env!("USER_BENCH_BIN"));
static USER_MIRROR: &[u8] = include_bytes!(env!("USER_MIRROR_BIN"));

#[no_mangle]
pub extern "C" fn kmain(_hart: usize, _dtb: usize) -> ! {
    uart::init();

    println!();
    println!("pith v{}", env!("CARGO_PKG_VERSION"));
    println!("hart {} booting on rv64", _hart);
    println!();

    mm::init();
    trap::init();

    // bench harness: bench + mirror, two endpoints.
    //   ep_a: bench -> mirror
    //   ep_b: mirror -> bench
    // bench measures syscall + ipc round-trip latency in cycles.
    let bench_pid  = sched::spawn("bench",  USER_BENCH);
    let mirror_pid = sched::spawn("mirror", USER_MIRROR);

    let ep_a   = ipc::alloc_endpoint().expect("no free endpoints");
    let ep_b   = ipc::alloc_endpoint().expect("no free endpoints");
    let notif  = ipc::alloc_notification().expect("no free notifs");
    sched::install_cap(bench_pid,  0, cap::Cap::Endpoint(ep_a));
    sched::install_cap(bench_pid,  1, cap::Cap::Endpoint(ep_b));
    sched::install_cap(bench_pid,  2, cap::Cap::Notification(notif));
    sched::install_cap(mirror_pid, 0, cap::Cap::Endpoint(ep_a));
    sched::install_cap(mirror_pid, 1, cap::Cap::Endpoint(ep_b));
    println!("[pith] endpoints ep_a=#{} ep_b=#{} + notif #{} wired to bench (pid {}) and mirror (pid {})",
             ep_a, ep_b, notif, bench_pid, mirror_pid);

    println!("[pith] entering scheduler");
    sched::start();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[pith] kernel panic: {}", info);
    sbi::shutdown();
}
