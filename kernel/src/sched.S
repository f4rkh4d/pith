// kernel context switch. saves the 14 callee-saved registers (ra, sp,
// s0..s11) of the OUTGOING task into *a0, then loads the same set out
// of *a1 for the INCOMING task and `ret`s. the ret jumps to whatever
// `ra` was loaded — the address where the incoming task last yielded.
//
// extern "C" fn context_switch(old: *mut KContext, new: *const KContext);
//
// KContext layout in kernel/src/sched.rs must match these offsets exactly.

.section .text
.globl context_switch
.align 4
context_switch:
    sd  ra,   0(a0)
    sd  sp,   8(a0)
    sd  s0,  16(a0)
    sd  s1,  24(a0)
    sd  s2,  32(a0)
    sd  s3,  40(a0)
    sd  s4,  48(a0)
    sd  s5,  56(a0)
    sd  s6,  64(a0)
    sd  s7,  72(a0)
    sd  s8,  80(a0)
    sd  s9,  88(a0)
    sd  s10, 96(a0)
    sd  s11,104(a0)

    ld  ra,   0(a1)
    ld  sp,   8(a1)
    ld  s0,  16(a1)
    ld  s1,  24(a1)
    ld  s2,  32(a1)
    ld  s3,  40(a1)
    ld  s4,  48(a1)
    ld  s5,  56(a1)
    ld  s6,  64(a1)
    ld  s7,  72(a1)
    ld  s8,  80(a1)
    ld  s9,  88(a1)
    ld  s10, 96(a1)
    ld  s11,104(a1)

    ret
