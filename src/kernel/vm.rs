use core::ptr::null_mut;
use crate::kalloc::KMEM;
use crate::{PA2PTE, PGROUNDDOWN, PTE2PA, PX};
use crate::memlayout::{KERNBASE, PHYSTOP, PLIC, TRAMPOLINE, UART0, VIRTIO0};
use crate::proc::proc_mapstacks;
use crate::riscv::{MAXVA, PageTable, PGSIZE, PGTBL_SIZE, Pte, PTE_R, PTE_V, PTE_W, PTE_X};
use crate::string::memset;

/*
 * the kernel's page table.
 */
static mut kernel_pagetable: Option<PageTable> = None;

const etext: [u8; 0] = [0; 0];  // kernel.ld sets this to end of kernel code.

const trampoline: [u8; 0]= [0; 0]; // trampoline.S

// Make a direct-map page table for the kernel.
fn kvmmake() -> PageTable {
    let pg: *mut u8 = null_mut();
    unsafe {
        let pg = KMEM.as_mut().unwrap().kalloc();
        memset(pg, 0, PGSIZE);
    }
    let mut kpgtbl = pg as PageTable;

    // uart registers
    kvmmap(&mut kpgtbl, UART0, UART0, PGSIZE, PTE_R | PTE_W);

    // virtio mmio disk interface
    kvmmap(&mut kpgtbl, VIRTIO0, VIRTIO0, PGSIZE, PTE_R | PTE_W);

    // PLIC
    kvmmap(&mut kpgtbl, PLIC, PLIC, 0x400000, PTE_R | PTE_W);

    let etext_addr = (&etext as *const usize) as usize;
    // map kernel text executable and read-only.
    kvmmap(&mut kpgtbl, KERNBASE, KERNBASE, etext_addr - KERNBASE, PTE_R | PTE_X);

    // map kernel data and the physical RAM we'll make use of.
    kvmmap(&mut kpgtbl, etext_addr, etext_addr, PHYSTOP - etext_addr, PTE_R | PTE_W);

    let trapoline_addr = (&trampoline as *const usize) as usize;
    // map the trampoline for trap entry/exit to
    // the highest virtual address in the kernel.
    kvmmap(&mut kpgtbl, TRAMPOLINE, trapoline_addr, PGSIZE, PTE_R | PTE_X);

    // allocate and map a kernel stack for each process.
    proc_mapstacks(&mut kpgtbl);

    kpgtbl
}

// Initialize the one kernel_pagetable
pub fn kvminit() {
    unsafe {
        kernel_pagetable = Some(kvmmake());
    }
}

// add a mapping to the kernel page table.
// only used when booting.
// does not flush TLB or enable paging.
pub fn kvmmap(kpgtbl: &mut PageTable, va: usize, pa: usize, sz: usize, perm: u64)
{
    if mappages(kpgtbl, va, pa, sz, perm) != 0 {
        panic!("kvmmap");
    }
}

// Create PTEs for virtual addresses starting at va that refer to
// physical addresses starting at pa. va and size might not
// be page-aligned. Returns 0 on success, -1 if walk() couldn't
// allocate a needed page-table page.
fn mappages(pagetable: &mut PageTable, va: usize, mut pa: usize, size: usize, perm: u64) -> i32 {
    if size == 0 {
        panic!("mappages: size");
    }

    let mut a: usize = PGROUNDDOWN!(va);
    let last: usize = PGROUNDDOWN!(va + size - 1);

    loop {
        let pte: Pte = walk(pagetable, a, 1).unwrap();
        if pte == 0 {
            return -1;
        }

        if (*Pte) & PTE_V == 1 {
            panic!("mappages: remap");
        }

        (*pte) = PA2PTE(pa) | perm | PTE_V;
        if a == last {
            break;
        }

        a += PGSIZE;
        pa += PGSIZE;
    }

    return 0;
}

// Return the address of the PTE in page table pagetable
// that corresponds to virtual address va.  If alloc!=0,
// create any required page-table pages.
//
// The risc-v Sv39 scheme has three levels of page-table
// pages. A page-table page contains 512 64-bit PTEs.
// A 64-bit virtual address is split into five fields:
//   39..63 -- must be zero.
//   30..38 -- 9 bits of level-2 index.
//   21..29 -- 9 bits of level-1 index.
//   12..20 -- 9 bits of level-0 index.
//    0..11 -- 12 bits of byte offset within the page.
fn walk(pagetable: &mut PageTable, va: usize, alloc: usize) -> Option<*Pte> {
    if va >= MAXVA {
        panic!("walk");
    }

    let mut curr_pgtbl = pagetable;
    for level in 2..0 {
        pte: Pte = curr_pgtbl[PX!(level, va)] as Pte;
        if pte & PTE_V {
            curr_pgtbl = (PTE2PA!(pte) as *const u8) as &mut PageTable;
        } else {
            unsafe {
                let next_level_pgtbl = KMEM.as_mut().unwrap().kalloc();
                if alloc == 0 || next_level_pgtbl.is_null() {
                    return None;
                }
                memset(next_level_pgtbl, 0, PGSIZE);
                curr_pgtbl = next_level_pgtbl as &mut PageTable;
                pte = PA2PTE!(curr_pgtbl as usize) | PTE_V;
            }
        }
    }

    Some(&(curr_pgtbl[PX!(0, va)]))
}


