// pith kernel. boots from opensbi at 0x80200000, sets up paging + traps,
// spawns user tasks, wires their initial capabilities, and hands off to
// the scheduler.

#![no_std]
#![no_main]
#![allow(clippy::missing_safety_doc)]

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

static USER_PING:  &[u8] = include_bytes!(env!("USER_PING_BIN"));
static USER_PING2: &[u8] = include_bytes!(env!("USER_PING2_BIN"));
static USER_PONG:  &[u8] = include_bytes!(env!("USER_PONG_BIN"));

#[no_mangle]
pub extern "C" fn kmain(_hart: usize, _dtb: usize) -> ! {
    uart::init();

    println!();
    println!("pith v{}", env!("CARGO_PKG_VERSION"));
    println!("hart {} booting on rv64", _hart);
    println!();

    mm::init();
    trap::init();

    // spawn three ipc partners: two senders, one receiver. v0.6's fifo
    // queue holds the second sender while the first rendezvouses, so
    // pong gets all 10 messages in producer-fifo order.
    let ping_pid  = sched::spawn("ping",  USER_PING);
    let ping2_pid = sched::spawn("ping2", USER_PING2);
    let pong_pid  = sched::spawn("pong",  USER_PONG);

    // allocate a kernel-side endpoint and install it as cap 0 in every
    // task that talks on it.
    let ep = ipc::alloc_endpoint().expect("no free endpoints");
    sched::install_cap(ping_pid,  0, cap::Cap::Endpoint(ep));
    sched::install_cap(ping2_pid, 0, cap::Cap::Endpoint(ep));
    sched::install_cap(pong_pid,  0, cap::Cap::Endpoint(ep));
    println!("[pith] endpoint #{} shared as cap 0 by pid {}, {} and {}",
             ep, ping_pid, ping2_pid, pong_pid);

    println!("[pith] entering scheduler");
    sched::start();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[pith] kernel panic: {}", info);
    sbi::shutdown();
}
