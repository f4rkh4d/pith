// pith kernel. boots from opensbi at 0x80200000, prints a banner over
// uart, sets up traps, kicks the first user task, services its syscalls.
//
// no async, no allocator, no surprises.

#![no_std]
#![no_main]
#![allow(clippy::missing_safety_doc)]

use core::arch::global_asm;
use core::panic::PanicInfo;

global_asm!(include_str!("boot.S"));
global_asm!(include_str!("trap.S"));

mod uart;
mod trap;
mod mm;
mod proc;
mod ipc;
mod syscall;
mod sbi;

/// kernel entry. called from boot.S after the stack and bss are set up.
/// `_hart` and `dtb` come from opensbi (a0, a1). hart parking is in asm,
/// so we always run on hart 0 here.
#[no_mangle]
pub extern "C" fn kmain(_hart: usize, _dtb: usize) -> ! {
    uart::init();

    println!();
    println!("pith v{}", env!("CARGO_PKG_VERSION"));
    println!("hart {} booting on rv64", _hart);
    println!();

    mm::init();
    trap::init();
    proc::init();

    println!("[pith] entering userspace");
    proc::run_first();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[pith] kernel panic: {}", info);
    sbi::shutdown();
}
