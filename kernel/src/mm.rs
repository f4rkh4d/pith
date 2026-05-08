// memory management. v0.1 ships:
//   1. a tiny bump page allocator over physical memory above the kernel
//   2. an Sv39 page table builder (kernel + first user task)
//   3. switch satp + sfence
//
// no kalloc beyond pages; no swap; no on-demand. faulting on a missing
// mapping kills the offender.

use core::arch::asm;
use core::ptr;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;

const PTE_V: u64 = 1 << 0;
const PTE_R: u64 = 1 << 1;
const PTE_W: u64 = 1 << 2;
const PTE_X: u64 = 1 << 3;
const PTE_U: u64 = 1 << 4;
const PTE_G: u64 = 1 << 5;

pub const PROT_R: u64 = PTE_R;
pub const PROT_RW: u64 = PTE_R | PTE_W;
pub const PROT_RX: u64 = PTE_R | PTE_X;
pub const PROT_RWX: u64 = PTE_R | PTE_W | PTE_X;

const SATP_SV39: u64 = 8u64 << 60;

extern "C" {
    static __kernel_end: u8;
}

// physical memory ends at 0x80000000 + RAM_SIZE. qemu-virt default is 128 MiB.
// kernel image lives in low ram. allocator hands out pages above
// __kernel_end. v0.1 only consumes the first ~few MiB so we don't even
// bother probing the device tree.
const PHYS_END: usize = 0x8000_0000 + 128 * 1024 * 1024;

static mut NEXT_FREE: usize = 0;

pub unsafe fn init_alloc() {
    let end = &__kernel_end as *const u8 as usize;
    NEXT_FREE = (end + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
}

pub fn alloc_page() -> Option<*mut u8> {
    unsafe {
        if NEXT_FREE + PAGE_SIZE > PHYS_END {
            return None;
        }
        let p = NEXT_FREE as *mut u8;
        NEXT_FREE += PAGE_SIZE;
        ptr::write_bytes(p, 0, PAGE_SIZE);
        Some(p)
    }
}

#[repr(C, align(4096))]
pub struct PageTable {
    entries: [u64; 512],
}

impl PageTable {
    pub fn zeroed() -> *mut PageTable {
        let p = alloc_page().expect("oom: page table");
        p as *mut PageTable
    }

    /// install one 4 KiB mapping va -> pa with the given permission bits.
    /// builds intermediate page tables on demand. va/pa must be page-aligned.
    pub unsafe fn map(&mut self, va: u64, pa: u64, perm: u64) {
        debug_assert!(va & 0xfff == 0);
        debug_assert!(pa & 0xfff == 0);

        let vpn = [
            ((va >> 12) & 0x1ff) as usize,
            ((va >> 21) & 0x1ff) as usize,
            ((va >> 30) & 0x1ff) as usize,
        ];

        let mut table = self as *mut PageTable;
        for level in (1..=2).rev() {
            let pte = &mut (*table).entries[vpn[level]];
            if *pte & PTE_V == 0 {
                let next = PageTable::zeroed();
                *pte = ((next as u64) >> 12) << 10 | PTE_V;
            }
            let next_pa = (*pte >> 10) << 12;
            table = next_pa as *mut PageTable;
        }
        (*table).entries[vpn[0]] = (pa >> 12) << 10 | perm | PTE_V;
    }

    /// install an identity mapping for [pa, pa + len). len rounds up.
    pub unsafe fn identity(&mut self, pa: u64, len: usize, perm: u64) {
        let mut p = pa & !(PAGE_SIZE as u64 - 1);
        let end = (pa + len as u64 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);
        while p < end {
            self.map(p, p, perm);
            p += PAGE_SIZE as u64;
        }
    }
}

static mut KERNEL_PT: *mut PageTable = core::ptr::null_mut();

pub fn init() {
    unsafe { init_alloc(); }

    // build the kernel page table: identity-map every region the kernel
    // touches. this is enough to flip satp without losing the program
    // counter (we keep executing kernel code).
    let pt = PageTable::zeroed();
    unsafe {
        (*pt).identity(0x1000_0000, PAGE_SIZE, PROT_RW);                 // uart mmio
        (*pt).identity(0x0c00_0000, 0x0040_0000, PROT_RW);               // PLIC
        (*pt).identity(0x8000_0000, 128 * 1024 * 1024, PROT_RWX);        // ram
        KERNEL_PT = pt;

        // turn on Sv39 paging.
        let satp = SATP_SV39 | ((pt as u64) >> 12);
        asm!(
            "csrw satp, {0}",
            "sfence.vma",
            in(reg) satp,
        );
    }
    crate::println!("[pith] paging on (sv39)");
}

pub fn kernel_pt() -> *mut PageTable {
    unsafe { KERNEL_PT }
}

/// build a fresh page table that contains the kernel mapping plus the
/// caller-supplied user mappings. the user task's image and stack pages
/// are mapped at the va supplied.
pub fn new_user_pt() -> *mut PageTable {
    let pt = PageTable::zeroed();
    unsafe {
        // share the kernel mapping. for v0.1 we duplicate the identity
        // map; production microkernels keep one shared kernel half and
        // swap only the user half. fine for now.
        (*pt).identity(0x1000_0000, PAGE_SIZE, PROT_RW);
        (*pt).identity(0x0c00_0000, 0x0040_0000, PROT_RW);
        (*pt).identity(0x8000_0000, 128 * 1024 * 1024, PROT_RWX);
    }
    pt
}

pub fn satp_for(pt: *mut PageTable) -> u64 {
    SATP_SV39 | ((pt as u64) >> 12)
}
