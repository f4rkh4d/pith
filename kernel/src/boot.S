// pith boot stub. opensbi jumps here in S-mode with:
//   a0 = hart id
//   a1 = device tree blob pointer
// our job: park non-bsp harts, set up the stack, zero bss, jump to rust.

.section .text.entry
.globl _start
_start:
    # park every hart that is not hart 0 in wfi.
    bnez    a0, _park

    # bsp: load the stack top address and switch to it.
    la      sp, __stack_top

    # zero bss. cleared per ELF convention.
    la      t0, __bss_start
    la      t1, __bss_end
1:  bgeu    t0, t1, 2f
    sd      zero, 0(t0)
    addi    t0, t0, 8
    j       1b
2:

    # jump to rust. a0 / a1 are preserved (hart id, dtb).
    call    kmain

    # kmain returns? halt cleanly.
_park:
    wfi
    j       _park
