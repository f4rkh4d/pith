// pith trap entry. one vector for both kernel and user traps; we
// distinguish on entry by reading sstatus.SPP.
//
// layout of TrapFrame matches kernel/src/trap.rs::TrapFrame exactly.
// 32 GP registers + sepc + sstatus + sscratch_user = 35 u64 = 280 bytes.
// stack must be 16-aligned per the riscv calling convention.

.equ FRAME_SIZE, 280

.altmacro
.macro SAVE_GP reg, off
    sd      x\reg, \off(sp)
.endm
.macro LOAD_GP reg, off
    ld      x\reg, \off(sp)
.endm

.section .text
.globl trap_entry
.align 4
trap_entry:
    # swap sscratch (= kernel sp for user traps, 0 for kernel-from-kernel
    # traps) with sp. if it was 0, we're already on the kernel stack and
    # need to reserve frame space ourselves.
    csrrw   sp, sscratch, sp

    bnez    sp, _from_user

_from_kernel:
    # restore sp (sscratch was 0 -> after swap, original sp is in sscratch).
    csrrw   sp, sscratch, sp
    addi    sp, sp, -FRAME_SIZE
    j       _save_state

_from_user:
    # sscratch held the kernel stack top; sp now points at it.
    addi    sp, sp, -FRAME_SIZE

_save_state:
    # save all 32 gp registers. x0 is hardwired zero, kept for layout.
    SAVE_GP 1, 8
    SAVE_GP 3, 24
    SAVE_GP 4, 32
    SAVE_GP 5, 40
    SAVE_GP 6, 48
    SAVE_GP 7, 56
    SAVE_GP 8, 64
    SAVE_GP 9, 72
    SAVE_GP 10, 80
    SAVE_GP 11, 88
    SAVE_GP 12, 96
    SAVE_GP 13, 104
    SAVE_GP 14, 112
    SAVE_GP 15, 120
    SAVE_GP 16, 128
    SAVE_GP 17, 136
    SAVE_GP 18, 144
    SAVE_GP 19, 152
    SAVE_GP 20, 160
    SAVE_GP 21, 168
    SAVE_GP 22, 176
    SAVE_GP 23, 184
    SAVE_GP 24, 192
    SAVE_GP 25, 200
    SAVE_GP 26, 208
    SAVE_GP 27, 216
    SAVE_GP 28, 224
    SAVE_GP 29, 232
    SAVE_GP 30, 240
    SAVE_GP 31, 248

    # sepc, sstatus
    csrr    t0, sepc
    sd      t0, 256(sp)
    csrr    t0, sstatus
    sd      t0, 264(sp)

    # save the user sp we displaced (currently in sscratch).
    csrr    t0, sscratch
    sd      t0, 16(sp)
    # zero sscratch so a kernel-mode re-entry takes the kernel-from-kernel
    # path on the next trap.
    csrw    sscratch, zero

    # call rust trap dispatcher with frame ptr in a0.
    mv      a0, sp
    call    trap_dispatch

    # return path. trap_dispatch may have changed registers in the frame,
    # restore them all.
    ld      t0, 256(sp)
    csrw    sepc, t0
    ld      t0, 264(sp)
    csrw    sstatus, t0

    # if we are returning to user (SPP=0), put the saved sp back into
    # sscratch so the next trap can find the kernel stack.
    li      t1, 1 << 8
    and     t1, t0, t1
    bnez    t1, _ret_kernel
_ret_user:
    # sscratch <- kernel sp (sp itself, before we leave it).
    addi    t1, sp, FRAME_SIZE
    csrw    sscratch, t1
_ret_kernel:

    LOAD_GP 1, 8
    LOAD_GP 3, 24
    LOAD_GP 4, 32
    LOAD_GP 5, 40
    LOAD_GP 6, 48
    LOAD_GP 7, 56
    LOAD_GP 8, 64
    LOAD_GP 9, 72
    LOAD_GP 10, 80
    LOAD_GP 11, 88
    LOAD_GP 12, 96
    LOAD_GP 13, 104
    LOAD_GP 14, 112
    LOAD_GP 15, 120
    LOAD_GP 16, 128
    LOAD_GP 17, 136
    LOAD_GP 18, 144
    LOAD_GP 19, 152
    LOAD_GP 20, 160
    LOAD_GP 21, 168
    LOAD_GP 22, 176
    LOAD_GP 23, 184
    LOAD_GP 24, 192
    LOAD_GP 25, 200
    LOAD_GP 26, 208
    LOAD_GP 27, 216
    LOAD_GP 28, 224
    LOAD_GP 29, 232
    LOAD_GP 30, 240
    LOAD_GP 31, 248

    # restore the saved sp last. for user-mode return this is the user
    # stack pointer; for kernel return we just bump sp past the frame.
    ld      sp, 16(sp)
    sret
