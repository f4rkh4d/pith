// trap dispatch. trap_entry (asm) saves the full register state into a
// TrapFrame on the kernel stack and calls trap_dispatch with the frame.
// we look at scause + stval to decide what to do, mutate the frame to
// pass return values, and let asm restore + sret back.

use core::arch::asm;
use crate::{println, sbi, sched, syscall};

/// quantum in mtime ticks. qemu-virt clocks `time` at 10 MHz so
/// 100 000 ticks = 10 ms. small enough to feel preemptive, large
/// enough that the timer overhead is irrelevant.
const TIMER_QUANTUM: u64 = 100_000;

/// register save area. layout matches trap.S exactly. don't reorder.
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct TrapFrame {
    pub regs: [u64; 32], // x0..x31. x0 unused but kept for indexed access.
    pub sepc: u64,       // off 256
    pub sstatus: u64,    // off 264
    pub _pad: u64,       // off 272 (keep frame 16-aligned at 280 bytes)
}

const SCAUSE_INTERRUPT: u64 = 1 << 63;
const SCAUSE_CODE: u64      = !SCAUSE_INTERRUPT;

const EXC_ECALL_U: u64 = 8;     // user environment call
const EXC_PAGE_FAULT_R: u64 = 13;
const EXC_PAGE_FAULT_W: u64 = 15;
const EXC_PAGE_FAULT_X: u64 = 12;

const INT_TIMER_S: u64 = 5;
const INT_EXTERNAL_S: u64 = 9;

pub fn init() {
    extern "C" { fn trap_entry(); }
    unsafe {
        // direct mode (low 2 bits = 0). sscratch starts at 0 so the
        // first kernel trap takes the from-kernel path; once we enter
        // user mode the scheduler will set sscratch = kernel stack top.
        asm!(
            "csrw stvec, {tv}",
            "csrw sscratch, zero",
            tv = in(reg) trap_entry as usize,
        );
        // let u-mode read cycle / time / instret directly via rdcycle etc.
        // bit 0 = CY, 1 = TM, 2 = IR. setting them all is harmless and
        // saves a syscall in user-side benchmarks.
        let counters: u64 = 0b111;
        asm!("csrw scounteren, {}", in(reg) counters);
        // enable the s-mode timer interrupt. opensbi already delegated
        // it to s-mode (mideleg.STI = 1), so flipping sie.STIE here is
        // enough — interrupts in u-mode are unmasked by default.
        let stie: u64 = 1 << 5;
        asm!("csrs sie, {}", in(reg) stie);
        // arm the first deadline so a runaway task always trips the
        // scheduler within one quantum.
        sbi::set_timer(read_time() + TIMER_QUANTUM);
    }
    println!("[pith] trap vector installed (timer quantum = {} ticks)", TIMER_QUANTUM);
}

#[no_mangle]
pub extern "C" fn trap_dispatch(frame: &mut TrapFrame) {
    let scause: u64;
    let stval: u64;
    unsafe {
        asm!(
            "csrr {0}, scause",
            "csrr {1}, stval",
            out(reg) scause,
            out(reg) stval,
        );
    }

    let is_int = scause & SCAUSE_INTERRUPT != 0;
    let code   = scause & SCAUSE_CODE;

    if is_int {
        match code {
            INT_TIMER_S => {
                // re-arm before yielding so we never miss a deadline
                // even if the next task burns kernel time too.
                sbi::set_timer(read_time() + TIMER_QUANTUM);
                sched::yield_now();
            }
            INT_EXTERNAL_S => {
                // PLIC routing not wired up yet. ignore.
            }
            other => {
                println!("[pith] unknown interrupt cause {}", other);
            }
        }
        return;
    }

    match code {
        EXC_ECALL_U => {
            // step over the ecall instruction so sret returns to the
            // next user instruction, not back into the syscall.
            frame.sepc = frame.sepc.wrapping_add(4);
            syscall::dispatch(frame);
        }
        EXC_PAGE_FAULT_R | EXC_PAGE_FAULT_W | EXC_PAGE_FAULT_X => {
            println!(
                "[pith] page fault: cause={} stval={:#x} sepc={:#x}",
                code, stval, frame.sepc
            );
            // for v0.1: kill. capabilities + on-demand mapping come later.
            sbi::shutdown();
        }
        other => {
            println!(
                "[pith] unhandled exception {} stval={:#x} sepc={:#x}",
                other, stval, frame.sepc
            );
            sbi::shutdown();
        }
    }
}

fn read_time() -> u64 {
    let t: u64;
    unsafe { asm!("csrr {0}, time", out(reg) t) };
    t
}
