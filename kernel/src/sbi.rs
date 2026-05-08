// minimal SBI (Supervisor Binary Interface) shim. opensbi hands us this
// when we boot, and we use a couple of calls to shut down cleanly + ask
// for a timer interrupt later. spec: riscv-non-isa.github.io/riscv-sbi-doc.

use core::arch::asm;

#[inline(always)]
pub fn ecall(eid: usize, fid: usize, a0: usize, a1: usize, a2: usize) -> (usize, usize) {
    let err: usize;
    let val: usize;
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inlateout("a0") a0 => err,
            inlateout("a1") a1 => val,
            in("a2") a2,
            options(nostack),
        );
    }
    (err, val)
}

/// reset extension (eid 0x53525354 "SRST"), system shutdown.
pub fn shutdown() -> ! {
    const SRST: usize = 0x53525354;
    let _ = ecall(SRST, 0, 0, 0, 0);
    // if SBI doesn't kill us, halt forever.
    loop {
        unsafe { asm!("wfi") }
    }
}

/// timer extension (eid 0x54494D45 "TIME"), schedule next stimer.
pub fn set_timer(stime_ticks: u64) {
    const TIME: usize = 0x54494D45;
    let _ = ecall(TIME, 0, stime_ticks as usize, 0, 0);
}
