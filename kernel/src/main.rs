// pith kernel. boots from opensbi at 0x80200000, sets up paging + traps,
// spawns one or more user tasks, hands control to the cooperative
// scheduler, and shuts down when the last task exits.

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
mod syscall;
mod sbi;

static USER_HELLO: &[u8] = include_bytes!(env!("USER_HELLO_BIN"));
static USER_ECHO:  &[u8] = include_bytes!(env!("USER_ECHO_BIN"));

#[no_mangle]
pub extern "C" fn kmain(_hart: usize, _dtb: usize) -> ! {
    uart::init();

    println!();
    println!("pith v{}", env!("CARGO_PKG_VERSION"));
    println!("hart {} booting on rv64", _hart);
    println!();

    mm::init();
    trap::init();

    sched::spawn("hello", USER_HELLO);
    sched::spawn("echo",  USER_ECHO);

    println!("[pith] entering scheduler");
    sched::start();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[pith] kernel panic: {}", info);
    sbi::shutdown();
}
