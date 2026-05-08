// process plumbing. v0.1 has exactly one user task; the user binary is
// embedded at compile time as a flat blob, mapped into the kernel page
// table at a fixed user VA, and we sret into it.
//
// no scheduler yet: the user task runs to completion or yields via the
// `yield` syscall (currently a no-op halt). preemptive scheduling lives
// behind a TODO until v0.2 wires the timer + a runqueue.

use core::arch::asm;
use core::ptr;
use crate::{mm, println, trap::TrapFrame};

extern "C" {
    static __stack_top: u8;
}

const USER_CODE_VA: u64  = 0x4000_0000;
const USER_STACK_VA: u64 = 0x4100_0000;
const USER_STACK_PAGES: usize = 4;            // 16 KiB

const SSTATUS_SPP: u64 = 1 << 8;              // 0 = U, 1 = S
const SSTATUS_SPIE: u64 = 1 << 5;
const SSTATUS_SIE: u64  = 1 << 1;
const SSTATUS_SUM: u64  = 1 << 18;

/// the only user binary we ship for v0.1. four hand-assembled RISC-V
/// instructions: `li a7, 1; ecall; li a7, 0; ecall`. that is "say-hi
/// (syscall 1), then exit (syscall 0)". in v0.2 this becomes a real
/// ELF loader against the user/* workspace crates; for v0.1 we keep
/// the demo path utterly transparent — every byte you see executes.
#[rustfmt::skip]
static USER_HELLO: &[u8] = &[
    0x93, 0x08, 0x10, 0x00,  // addi a7, zero, 1     (syscall = HI)
    0x73, 0x00, 0x00, 0x00,  // ecall
    0x93, 0x08, 0x00, 0x00,  // addi a7, zero, 0     (syscall = EXIT)
    0x73, 0x00, 0x00, 0x00,  // ecall
];

#[derive(Default)]
pub struct Process {
    pub frame: TrapFrame,
    pub kstack_top: u64,
    pub user_entry: u64,
    pub user_sp: u64,
}

static mut FIRST: Process = Process {
    frame: TrapFrame { regs: [0; 32], sepc: 0, sstatus: 0, _pad: 0 },
    kstack_top: 0,
    user_entry: 0,
    user_sp: 0,
};

pub fn init() {
    println!("[pith] proc init: {} bytes user binary", USER_HELLO.len());
}

/// build the first user task and sret into it. never returns to the
/// caller; the only way back is through trap_dispatch.
pub fn run_first() -> ! {
    unsafe {
        let pt = &mut *mm::kernel_pt();

        // map enough user code pages to fit the embedded binary.
        let pages = (USER_HELLO.len() + mm::PAGE_SIZE - 1) / mm::PAGE_SIZE;
        for i in 0..pages {
            let pa = mm::alloc_page().expect("oom: user code");
            let chunk_off = i * mm::PAGE_SIZE;
            let to_copy = core::cmp::min(mm::PAGE_SIZE, USER_HELLO.len() - chunk_off);
            ptr::copy_nonoverlapping(
                USER_HELLO.as_ptr().add(chunk_off),
                pa,
                to_copy,
            );
            pt.map(
                USER_CODE_VA + (i * mm::PAGE_SIZE) as u64,
                pa as u64,
                mm::PROT_RWX | (1 << 4), // PTE_U
            );
        }

        // user stack.
        for i in 0..USER_STACK_PAGES {
            let pa = mm::alloc_page().expect("oom: user stack");
            pt.map(
                USER_STACK_VA + (i * mm::PAGE_SIZE) as u64,
                pa as u64,
                mm::PROT_RW | (1 << 4),
            );
        }

        // sfence so the new mappings take effect for U-mode.
        asm!("sfence.vma");

        let user_sp_top = USER_STACK_VA + (USER_STACK_PAGES * mm::PAGE_SIZE) as u64;

        FIRST.user_entry = USER_CODE_VA;
        FIRST.user_sp    = user_sp_top;

        // sstatus to install on sret:
        //   SPP = 0   (return to U-mode)
        //   SPIE = 1  (re-enable interrupts after sret)
        //   SUM  = 1  (kernel may touch user pages, useful when copy_in/out lands)
        let sstatus_in: u64;
        asm!("csrr {0}, sstatus", out(reg) sstatus_in);
        let sstatus = (sstatus_in & !SSTATUS_SPP) | SSTATUS_SPIE | SSTATUS_SUM;

        // sscratch must hold the kernel stack top *before* we leave
        // S-mode. trap_entry swaps sp <-> sscratch on user trap to
        // recover the kernel stack.
        let kstack_top = &__stack_top as *const u8 as u64;

        // jump. inline asm so we don't accidentally clobber sp before sret.
        asm!(
            "csrw sscratch, {kstack}",
            "csrw sepc, {entry}",
            "csrw sstatus, {sstatus}",
            "mv  sp, {usp}",
            "sret",
            kstack  = in(reg) kstack_top,
            entry   = in(reg) USER_CODE_VA,
            sstatus = in(reg) sstatus,
            usp     = in(reg) user_sp_top,
            options(noreturn),
        );
    }
}
