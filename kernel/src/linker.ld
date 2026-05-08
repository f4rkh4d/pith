/* pith kernel linker script.
   loaded by opensbi at 0x80200000 (qemu-virt sbi default jump address).
   the boot stub at .text.entry sits first so the entry point is exactly
   the load address, no header thunk in the way.
*/

OUTPUT_ARCH(riscv)
ENTRY(_start)

KERNEL_BASE = 0x80200000;

SECTIONS {
    . = KERNEL_BASE;

    .text : ALIGN(4K) {
        PROVIDE(__text_start = .);
        KEEP(*(.text.entry));
        *(.text .text.*);
        PROVIDE(__text_end = .);
    }

    .rodata : ALIGN(4K) {
        PROVIDE(__rodata_start = .);
        *(.rodata .rodata.*);
        *(.srodata .srodata.*);
        PROVIDE(__rodata_end = .);
    }

    .data : ALIGN(4K) {
        PROVIDE(__data_start = .);
        *(.data .data.*);
        *(.sdata .sdata.*);
        PROVIDE(__data_end = .);
    }

    .bss : ALIGN(4K) {
        PROVIDE(__bss_start = .);
        *(.sbss .sbss.*);
        *(.bss .bss.*);
        PROVIDE(__bss_end = .);
    }

    /* boot stack reserved at the end of the image; the entry stub points
       sp here before any rust code runs. one page per hart, hart0 only
       in v0.1. */
    . = ALIGN(16);
    PROVIDE(__stack_bottom = .);
    . += 64K;
    PROVIDE(__stack_top = .);

    /* the rest of physical memory is the kernel heap region; mm.rs
       discovers it at runtime by scanning past __kernel_end. */
    PROVIDE(__kernel_end = .);

    /DISCARD/ : {
        *(.eh_frame .eh_frame.*);
        *(.note .note.*);
        *(.comment);
    }
}
