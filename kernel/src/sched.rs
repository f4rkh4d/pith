// cooperative scheduler. v0.3 ships exactly this:
//
//   - a fixed-size process table (MAX_PROCS = 4)
//   - one process per user binary; each owns its own page table, kernel
//     stack, and trap frame
//   - SYS_YIELD enters the scheduler, which picks the next READY task
//     and context-switches to it
//   - first dispatch is the same code path as a yield: kmain creates
//     all processes, then yields into the scheduler from a synthetic
//     "init" context and never returns
//
// preemption (timer-driven) lands in v0.4. once the scheduler is
// re-entered from any other path (timer interrupt instead of yield)
// the rest of this module already does the right thing.

use core::arch::asm;
use core::ptr;
use crate::{cap, ipc, mm, println, trap::TrapFrame};

/// matches FRAME_SIZE in trap.S. the trap frame for a task lives at
/// kstack_top - FRAME_SIZE while the task is paused (between the trap
/// entry and trap.S's epilogue). modifying it from here lands changes
/// in the registers sret will load when the task resumes.
const FRAME_SIZE: u64 = 280;

extern "C" {
    static __stack_top: u8;
    fn context_switch(old: *mut KContext, new: *const KContext);
}

pub const MAX_PROCS: usize = 4;
pub const KSTACK_PAGES: usize = 4;            // 16 KiB per task

const USER_CODE_VA: u64  = 0x4000_0000;
const USER_STACK_VA: u64 = 0x4100_0000;
const USER_STACK_PAGES: usize = 4;

const SSTATUS_SPP: u64  = 1 << 8;
const SSTATUS_SPIE: u64 = 1 << 5;
const SSTATUS_SUM: u64  = 1 << 18;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum State {
    Unused,
    Ready,
    Running,
    BlockedOnSend(ipc::EndpointId),
    BlockedOnRecv(ipc::EndpointId),
    BlockedOnNotif(ipc::NotifId),
    Exited,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct KContext {
    pub ra:   u64,
    pub sp:   u64,
    pub s:    [u64; 12],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Process {
    pub pid:        u32,
    pub state:      State,
    pub kctx:       KContext,
    pub kstack_top: u64,
    pub pt:         *mut mm::PageTable,
    pub frame:      TrapFrame,
    pub user_entry: u64,
    pub user_sp:    u64,
    pub caps:       cap::CapTable,
    /// the slot a blocked receiver wants any incoming granted cap
    /// installed at. updated by ipc::recv() right before parking.
    pub pending_grant_dst: u8,
}

const EMPTY_PROC: Process = Process {
    pid: 0,
    state: State::Unused,
    kctx: KContext { ra: 0, sp: 0, s: [0; 12] },
    kstack_top: 0,
    pt: ptr::null_mut(),
    frame: TrapFrame { regs: [0; 32], sepc: 0, sstatus: 0, _pad: 0 },
    user_entry: 0,
    user_sp: 0,
    caps: cap::CapTable::empty(),
    pending_grant_dst: 0xff,
};

static mut PROCS: [Process; MAX_PROCS] = [EMPTY_PROC; MAX_PROCS];

static mut CURRENT: usize = MAX_PROCS;     // sentinel "no current"
static mut SCHED_CTX: KContext = KContext { ra: 0, sp: 0, s: [0; 12] };

pub fn current() -> Option<&'static mut Process> {
    unsafe {
        if CURRENT < MAX_PROCS && PROCS[CURRENT].state != State::Unused {
            Some(&mut PROCS[CURRENT])
        } else {
            None
        }
    }
}

/// install a new user task built from a flat blob. returns its pid.
/// the kernel page table is reused as a template for the per-task
/// page table; user pages get PTE_U.
pub fn spawn(name: &str, user_blob: &[u8]) -> u32 {
    unsafe {
        let slot = (0..MAX_PROCS)
            .find(|&i| PROCS[i].state == State::Unused)
            .expect("proc table full");

        let pid = (slot as u32) + 1;
        let pt  = mm::new_user_pt();

        // copy the flat blob into freshly allocated pages and map them.
        let pages = (user_blob.len() + mm::PAGE_SIZE - 1) / mm::PAGE_SIZE;
        for i in 0..pages {
            let pa = mm::alloc_page().expect("oom: user code");
            let off = i * mm::PAGE_SIZE;
            let len = core::cmp::min(mm::PAGE_SIZE, user_blob.len() - off);
            ptr::copy_nonoverlapping(user_blob.as_ptr().add(off), pa, len);
            (*pt).map(
                USER_CODE_VA + (i * mm::PAGE_SIZE) as u64,
                pa as u64,
                mm::PROT_RWX | (1 << 4),
            );
        }

        // user stack.
        for i in 0..USER_STACK_PAGES {
            let pa = mm::alloc_page().expect("oom: user stack");
            (*pt).map(
                USER_STACK_VA + (i * mm::PAGE_SIZE) as u64,
                pa as u64,
                mm::PROT_RW | (1 << 4),
            );
        }
        let user_sp_top = USER_STACK_VA + (USER_STACK_PAGES * mm::PAGE_SIZE) as u64;

        // kernel stack: KSTACK_PAGES contiguous pages from the bump
        // allocator. they're already in the kernel identity map, so no
        // mapping work; we just need the high address.
        let mut kstack_lo: u64 = 0;
        for i in 0..KSTACK_PAGES {
            let p = mm::alloc_page().expect("oom: kstack") as u64;
            if i == 0 { kstack_lo = p; }
            // pages happen to be contiguous because the bump allocator
            // never frees; assert the invariant.
            debug_assert!(p == kstack_lo + (i * mm::PAGE_SIZE) as u64);
        }
        let kstack_top = kstack_lo + (KSTACK_PAGES * mm::PAGE_SIZE) as u64;

        // initial trap frame: state we'll feel "as if" the task was just
        // about to leave kernel via sret. bootstrap_trampoline below loads
        // it into registers and srets to user.
        let mut frame = TrapFrame::default();
        frame.sepc    = USER_CODE_VA;
        frame.regs[2] = user_sp_top;          // x2 = sp
        frame.sstatus = SSTATUS_SPIE | SSTATUS_SUM; // SPP=0 (return to U), SUM=1, IE on after sret

        // initial KContext: when sched first switches into this task it
        // ret's into bootstrap_trampoline with sp at our kstack top.
        let mut kctx = KContext::default();
        kctx.ra = bootstrap_trampoline as usize as u64;
        kctx.sp = kstack_top;

        PROCS[slot] = Process {
            pid,
            state: State::Ready,
            kctx,
            kstack_top,
            pt,
            frame,
            user_entry: USER_CODE_VA,
            user_sp:    user_sp_top,
            caps:       cap::CapTable::empty(),
            pending_grant_dst: 0xff,
        };

        println!("[sched] spawned {} as pid {} ({} bytes)", name, pid, user_blob.len());
        pid
    }
}

/// initial kernel-mode entry for a freshly spawned task. when the
/// scheduler context-switches into a new process for the first time,
/// `ret` lands here. switch_to_runtime has already set satp + sscratch
/// for this task; we just sret to its initial trap frame.
unsafe extern "C" fn bootstrap_trampoline() -> ! {
    let p = current().expect("no current task on bootstrap");
    sret_to_frame(&p.frame);
}

/// install the per-task runtime state right before context_switch. both
/// satp (so the right user mappings are live) and sscratch (so the
/// next u-mode trap finds the right kernel stack) are set here.
unsafe fn install_runtime(idx: usize) {
    let p     = &PROCS[idx];
    let satp  = mm::satp_for(p.pt);
    let stack = p.kstack_top;
    asm!(
        "csrw sscratch, {scratch}",
        "csrw satp,     {satp}",
        "sfence.vma",
        scratch = in(reg) stack,
        satp    = in(reg) satp,
    );
}

/// load a TrapFrame into registers and sret. used for the very first
/// launch of a task (bootstrap_trampoline). subsequent resumptions go
/// through trap.S's epilogue instead of this path.
///
/// the frame pointer rides in a *callee-saved* register (s11) so the
/// load instructions that overwrite the temporaries + arg registers do
/// not clobber it. csr writes for sepc/sstatus happen before the gp
/// loads for the same reason — once t0/t1/etc are loaded with frame
/// data, we cannot reuse them as scratch.
#[inline(never)]
unsafe extern "C" fn sret_to_frame(frame: &TrapFrame) -> ! {
    asm!(
        "ld t0, 256(s11)",   // sepc
        "csrw sepc, t0",
        "ld t0, 264(s11)",   // sstatus
        "csrw sstatus, t0",
        "ld ra,   8(s11)",
        "ld gp,  24(s11)",
        "ld tp,  32(s11)",
        "ld t0,  40(s11)",
        "ld t1,  48(s11)",
        "ld t2,  56(s11)",
        "ld s0,  64(s11)",
        "ld s1,  72(s11)",
        "ld a0,  80(s11)",
        "ld a1,  88(s11)",
        "ld a2,  96(s11)",
        "ld a3, 104(s11)",
        "ld a4, 112(s11)",
        "ld a5, 120(s11)",
        "ld a6, 128(s11)",
        "ld a7, 136(s11)",
        "ld s2, 144(s11)",
        "ld s3, 152(s11)",
        "ld s4, 160(s11)",
        "ld s5, 168(s11)",
        "ld s6, 176(s11)",
        "ld s7, 184(s11)",
        "ld s8, 192(s11)",
        "ld s9, 200(s11)",
        "ld s10,208(s11)",
        // s11 still holds {f}; load it last, alongside sp.
        "ld t3, 224(s11)",
        "ld t4, 232(s11)",
        "ld t5, 240(s11)",
        "ld t6, 248(s11)",
        "ld sp,  16(s11)",
        "ld s11,216(s11)",
        "sret",
        in("s11") frame as *const _ as u64,
        options(noreturn),
    );
}

/// hand control to the next runnable task. called from the scheduler
/// loop and from SYS_YIELD.
pub fn yield_now() {
    unsafe {
        let cur = CURRENT;
        let next = pick_next(cur);
        if next == MAX_PROCS {
            return;
        }
        if cur < MAX_PROCS && PROCS[cur].state == State::Running {
            PROCS[cur].state = State::Ready;
        }
        PROCS[next].state = State::Running;
        CURRENT = next;

        let old_ctx: *mut KContext = if cur < MAX_PROCS {
            &mut PROCS[cur].kctx
        } else {
            &raw mut SCHED_CTX
        };
        let new_ctx: *const KContext = &PROCS[next].kctx;
        install_runtime(next);
        context_switch(old_ctx, new_ctx);
    }
}

/// mark the current task as exited and switch away. called from
/// SYS_EXIT in syscall.rs.
pub fn exit_current(code: u64) -> ! {
    unsafe {
        let cur = CURRENT;
        if cur < MAX_PROCS {
            let pid = PROCS[cur].pid;
            ipc::drop_waiters(pid);
            PROCS[cur].state = State::Exited;
            println!("[sched] pid {} exited (code {})", pid, code);
        }
        let next = pick_next(MAX_PROCS);
        if next == MAX_PROCS {
            crate::sbi::shutdown();
        }
        PROCS[next].state = State::Running;
        CURRENT = next;

        // we're abandoning the current task; throw away its saved kctx
        // by writing into a stack-local sink. context_switch then loads
        // the next task's kctx and never comes back here.
        let mut sink = KContext::default();
        let new_ctx: *const KContext = &PROCS[next].kctx;
        install_runtime(next);
        context_switch(&mut sink, new_ctx);
        unreachable!();
    }
}

/// round-robin next-runnable-after(skip).
unsafe fn pick_next(skip: usize) -> usize {
    for i in 1..=MAX_PROCS {
        let idx = if skip < MAX_PROCS { (skip + i) % MAX_PROCS } else { i - 1 };
        if PROCS[idx].state == State::Ready {
            return idx;
        }
    }
    MAX_PROCS
}

/// pid (1-based) of the currently running task.
pub fn current_pid() -> u32 {
    unsafe {
        if CURRENT < MAX_PROCS { PROCS[CURRENT].pid } else { 0 }
    }
}

/// pointer to a task's saved trap frame on its kstack. valid whenever
/// the task is paused (which is most of the time from the kernel's
/// perspective). modifying this writes the registers sret will load.
fn frame_of(idx: usize) -> *mut TrapFrame {
    unsafe { (PROCS[idx].kstack_top - FRAME_SIZE) as *mut TrapFrame }
}

fn idx_for(pid: u32) -> Option<usize> {
    unsafe {
        for i in 0..MAX_PROCS {
            if PROCS[i].pid == pid && PROCS[i].state != State::Unused {
                return Some(i);
            }
        }
        None
    }
}

/// access the current task's capability table. used by syscall
/// handlers to resolve cap handles to kernel objects.
pub fn current_caps_mut() -> &'static mut cap::CapTable {
    unsafe { &mut PROCS[CURRENT].caps }
}

/// initial cap installation. used by kmain to wire startup endpoints
/// before the scheduler runs.
pub fn install_cap(pid: u32, slot: usize, c: cap::Cap) {
    unsafe {
        let idx = idx_for(pid).expect("install_cap: bad pid");
        PROCS[idx].caps.install(slot, c).expect("install_cap: bad slot");
    }
}

/// block the current task on a send. saves its kctx via context_switch,
/// returns to scheduler, and (eventually) is resumed by recv() via
/// wake_with_status. when this function "returns" the task is back on
/// the cpu, frame on kstack already mutated with the ipc reply.
pub fn block_on_send(ep: ipc::EndpointId) {
    unsafe {
        let cur = CURRENT;
        PROCS[cur].state = State::BlockedOnSend(ep);
        switch_away_from(cur);
    }
}

pub fn block_on_recv(ep: ipc::EndpointId) {
    unsafe {
        let cur = CURRENT;
        PROCS[cur].state = State::BlockedOnRecv(ep);
        switch_away_from(cur);
    }
}

pub fn block_on_notif(n: ipc::NotifId) {
    unsafe {
        let cur = CURRENT;
        PROCS[cur].state = State::BlockedOnNotif(n);
        switch_away_from(cur);
    }
}

/// pop the current off the cpu, find next runnable, switch. shared
/// between block_on_send / block_on_recv and the timer/yield path.
unsafe fn switch_away_from(cur: usize) {
    let next = pick_next(cur);
    if next == MAX_PROCS {
        // nobody runnable. v0.5 behaviour: panic — a real micro-kernel
        // would idle here. easier to debug a panic than a silent wfi.
        println!("[sched] deadlock: no runnable task while pid {} blocked",
                 PROCS[cur].pid);
        crate::sbi::shutdown();
    }
    PROCS[next].state = State::Running;
    CURRENT = next;

    let old_ctx: *mut KContext = &mut PROCS[cur].kctx;
    let new_ctx: *const KContext = &PROCS[next].kctx;
    install_runtime(next);
    context_switch(old_ctx, new_ctx);
}

/// wake a blocked task and stash a status word in its a0 register so
/// when it resumes the syscall handler returns that value to user.
pub fn wake_with_status(pid: u32, a0: u64) {
    unsafe {
        let idx = idx_for(pid).expect("wake_with_status: bad pid");
        let frame = &mut *frame_of(idx);
        frame.regs[10] = a0; // a0
        PROCS[idx].state = State::Ready;
    }
}

/// fast-path delivery of a 4-word ipc message to a blocked receiver.
/// also installs an optional granted capability into the receiver's
/// stashed grant_dst slot (set when they called recv()).
pub fn deliver_to(pid: u32, msg: ipc::Message, grant: Option<cap::Cap>) {
    unsafe {
        let idx = idx_for(pid).expect("deliver_to: bad pid");
        if let Some(g) = grant {
            let dst = PROCS[idx].pending_grant_dst as usize;
            PROCS[idx].caps.install(dst, g).ok();
        }
        let frame = &mut *frame_of(idx);
        frame.regs[10] = msg.label;
        frame.regs[11] = msg.words[0];
        frame.regs[12] = msg.words[1];
        frame.regs[13] = msg.words[2];
        PROCS[idx].state = State::Ready;
    }
}

/// stash the receiver's grant destination slot. read back by deliver_to
/// when a sender finally rendezvouses with the parked receiver.
pub fn set_current_grant_dst(slot: u8) {
    unsafe {
        if CURRENT < MAX_PROCS {
            PROCS[CURRENT].pending_grant_dst = slot;
        }
    }
}

/// kmain calls this once after spawning all initial tasks. it never
/// returns; the scheduler runs forever until the last task exits.
pub fn start() -> ! {
    unsafe {
        let next = pick_next(MAX_PROCS);
        assert!(next < MAX_PROCS, "no ready task at sched::start()");
        PROCS[next].state = State::Running;
        CURRENT = next;
        let new_ctx: *const KContext = &PROCS[next].kctx;
        install_runtime(next);
        context_switch(&raw mut SCHED_CTX, new_ctx);
    }
    crate::sbi::shutdown();
}

/// called from trap dispatch when the current task is in s-mode at the
/// time of the trap. after trap_dispatch saves the frame onto the
/// kstack, we copy it into the Process struct so the next yield can
/// resume from there.
pub fn save_frame(frame: &TrapFrame) {
    unsafe {
        if CURRENT < MAX_PROCS {
            PROCS[CURRENT].frame = *frame;
        }
    }
}

/// the inverse of save_frame: copies the current task's saved frame
/// back to the live frame on the kernel stack so trap.S restores it.
pub fn load_frame(frame: &mut TrapFrame) {
    unsafe {
        if CURRENT < MAX_PROCS {
            *frame = PROCS[CURRENT].frame;
        }
    }
}
