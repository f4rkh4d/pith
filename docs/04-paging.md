# 04 paging

riscv64 ships three page-table formats: `sv39`, `sv48`, `sv57`. pith
uses sv39 because it covers 512 GiB, qemu-virt has 128 MiB, and three
levels keep the page-table walker code under 50 lines.

## sv39 layout

```
virtual address (39 bits used out of 64):
[ ext sign | vpn[2] (9) | vpn[1] (9) | vpn[0] (9) | page off (12) ]
```

each level is a 512-entry table of 8-byte page-table entries (PTEs).
the high 25 bits must sign-extend the 39th, like ARM TBI. we never
hand out user VAs above `0x4000_0000` so this never bites us.

a leaf PTE looks like:

```
[ ppn[2] | ppn[1] | ppn[0] |  reserved | dabe | uxwrv ]
   26b      9b      9b        7b         5b    5b
```

bit names:

- `V` valid
- `R/W/X` read / write / execute
- `U` accessible from u-mode (without this bit, only s-mode can read it)
- `G` global (across address spaces)
- `A/D` accessed / dirty (set by hw on first touch)

## the builder

[`mm.rs`](../kernel/src/mm.rs) exposes `PageTable::map(va, pa, perm)`.
walk the levels top-down, allocate intermediate tables on demand,
plant a leaf at level 0:

```rust
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
```

`PageTable::zeroed()` calls `alloc_page` (the bump allocator), zeroes
the page, returns a pointer. nothing to free; v0.1 never tears down.

## turning paging on

```rust
let satp = SATP_SV39 | ((pt as u64) >> 12);
asm!("csrw satp, {0}", "sfence.vma", in(reg) satp);
```

`SATP_SV39 = 8 << 60`. after this csr write the cpu walks `pt` for
every load/store/fetch. because we identity-mapped the kernel image
before flipping satp, nothing crashes — the next instruction the
fetch unit reads still resolves to the same physical address it would
have without paging.

## kernel half vs user half

we don't have a clean kernel/user split yet. one shared page table
gets the kernel image identity-mapped (no `U`) and the user pages
mapped at `0x4000_0000` and up (`U` set). when sret drops to u-mode,
the cpu enforces `U`: a user load from `0x80200000` faults.

v0.4 splits this: every task carries its own page table, the kernel
half is a single `Arc<PageTable>` shared between all tasks, and switching
tasks only swaps the user half.
