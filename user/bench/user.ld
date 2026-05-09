/* user-task linker script. the kernel maps this image at 0x40000000.
   we keep a single contiguous .text-then-data load so the kernel's
   flat-blob loader doesn't have to parse program headers. */

OUTPUT_ARCH(riscv)
ENTRY(_start)

USER_BASE = 0x40000000;

SECTIONS {
    . = USER_BASE;

    .text : ALIGN(4K) {
        KEEP(*(.text.entry));
        *(.text .text.*);
    }

    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*);
        *(.srodata .srodata.*);
    }

    .data : ALIGN(4K) {
        *(.data .data.*);
        *(.sdata .sdata.*);
    }

    .bss : ALIGN(4K) {
        *(.sbss .sbss.*);
        *(.bss .bss.*);
    }

    /DISCARD/ : {
        *(.eh_frame .eh_frame.*);
        *(.note .note.*);
        *(.comment);
    }
}
