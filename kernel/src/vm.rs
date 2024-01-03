use crate::kalloc::KMEM;
use crate::{MAKE_SATP, PA2PTE, PGROUNDDOWN, PGROUNDUP, printf, PTE2PA, PTE_FLAGS, PX};
use crate::memlayout::{KERNBASE, PHYSTOP, PLIC, TRAMPOLINE, UART0, VIRTIO0};
use crate::proc::proc_mapstacks;
use crate::riscv::{MAXVA, PageTable, PGSIZE, Pte, PTE_R, PTE_U, PTE_V, PTE_W, PTE_X, sfence_vma, w_satp};
use crate::string::{memmove, memset};

/*
 * the kernel's page table.
 */
pub static mut KERNEL_PAGETABLE: Option<&'static PageTable> = None;

extern {
    static etext: u8;  // kernel.ld sets this to end of kernel code.
    static trampoline: u8; // trampoline.S
}

// Make a direct-map page table for the kernel.
fn kvmmake<'a>() -> &'a PageTable {
    let kpgtbl = unsafe {
        let pg: *mut PageTable = KMEM.kalloc();
        if pg.is_null() {
            panic!("failed to alloc for root page table");
        }
        memset(pg as *mut u8, 0, PGSIZE);
        pg.as_mut().unwrap()
    };
    // printf!("Root Page Table Allocated.\n");

    // uart registers
    kvmmap(kpgtbl, UART0, UART0, PGSIZE, PTE_R | PTE_W);
    // printf!("UART0 Mapped.\n");

    // virtio mmio disk interface
    kvmmap(kpgtbl, VIRTIO0, VIRTIO0, PGSIZE, PTE_R | PTE_W);
    // printf!("VIRTIO0 Mapped.\n");

    // PLIC
    kvmmap(kpgtbl, PLIC, PLIC, 0x400000, PTE_R | PTE_W);
    // printf!("PLIC Mapped.\n");

    let etext_addr = (unsafe { &etext } as *const u8).expose_addr();
    // map kernel text executable and read-only.
    kvmmap(kpgtbl, KERNBASE, KERNBASE, etext_addr - KERNBASE, PTE_R | PTE_X);
    // printf!("etext_addr: {:x}, KERNBASE: {:x}, PHYSTOP: {:x}, size: {}\n", etext_addr, KERNBASE, PHYSTOP, etext_addr - KERNBASE);
    // printf!("KERNBASE Mapped.\n");

    // map kernel data and the physical RAM we'll make use of.
    kvmmap(kpgtbl, etext_addr, etext_addr, PHYSTOP - etext_addr, PTE_R | PTE_W);
    // printf!("etext_addr Mapped.\n");

    let trapoline_addr = (unsafe { &trampoline } as *const u8).expose_addr();
    // map the trampoline for trap entry/exit to
    // the highest virtual address in the kernel.
    kvmmap(kpgtbl, TRAMPOLINE, trapoline_addr, PGSIZE, PTE_R | PTE_X);
    // printf!("TRAMPOLINE Mapped.\n");

    // allocate and map a kernel stack for each process.
    proc_mapstacks(kpgtbl);
    // printf!("Proc Kernel Stack Mapped.\n");

    kpgtbl
}

// Initialize the one KERNEL_PAGETABLE
pub fn kvminit() {
    unsafe {
        KERNEL_PAGETABLE = Some(kvmmake());
    }
}

// add a mapping to the kernel page table.
// only used when booting.
// does not flush TLB or enable paging.
pub fn kvmmap(kpgtbl: &mut PageTable, va: usize, pa: usize, sz: usize, perm: usize)
{
    if mappages(kpgtbl, va, pa, sz, perm) != 0 {
        panic!("kvmmap");
    }
}

// Create PTEs for virtual addresses starting at va that refer to
// physical addresses starting at pa. va and size might not
// be page-aligned. Returns 0 on success, -1 if walk() couldn't
// allocate a needed page-table page.
pub fn mappages(pagetable: &mut PageTable, va: usize, mut pa: usize, size: usize, perm: usize) -> i32 {
    if size == 0 {
        panic!("mappages: size");
    }

    let mut a: usize = PGROUNDDOWN!(va);
    let last: usize = PGROUNDDOWN!(va + size - 1);
    // printf!("a: {:x}, last: {:x}\n\n", a, last);

    loop {
        let pte: Option<&mut Pte> = walk(pagetable, a, 1);
        if pte.is_none() {
            return -1;
        }

        let pte = pte.unwrap();
        if pte.0 & PTE_V == 1 {
            printf!("a: {:x}, Pte: {:x}\n", a, pte.0);
            panic!("mappages: remap");
        }

        (*pte) = Pte(PA2PTE!(pa) | perm | PTE_V);
        if a == last {
            break;
        }

        a += PGSIZE;
        pa += PGSIZE;
    }
    return 0;
}

// Remove npages of mappings starting from va. va must be
// page-aligned. The mappings must exist.
// Optionally free the physical memory.
pub fn uvmunmap(pagetable: &mut PageTable, va: usize, npages: usize, do_free: bool) {
    if (va % PGSIZE) != 0 {
        panic!("uvmunmap: not aligned");
    }

    for a in (va..npages * PGSIZE).step_by(PGSIZE) {
        match walk(pagetable, a, 0) {
            None => panic!("uvmunmap: walk"),
            Some(pte) => {
                if pte.0 & PTE_V == 1 {
                    panic!("uvmunmap: not mapped");
                }

                if PTE_FLAGS!(pte.0) == PTE_V {
                    panic!("uvmunmap: not a leaf");
                }

                if do_free {
                    let pa = PTE2PA!(pte.0);
                    unsafe { KMEM.kfree(pa as *mut PageTable); }
                }
                *pte = Pte(0);
            }
        }
    }
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
fn walk(pagetable: &mut PageTable, va: usize, alloc: usize) -> Option<&mut Pte> {
    if va >= MAXVA {
        panic!("walk");
    }

    let mut curr_pgtbl = pagetable;
    for level in (1..3).rev() {
        let pte = &mut (curr_pgtbl.0)[PX!(level, va)];
        if pte.0 & PTE_V  == PTE_V {
            unsafe { curr_pgtbl = (PTE2PA!(pte.0) as *mut PageTable).as_mut().unwrap(); }
        } else {
            unsafe {
                if alloc == 0 {
                    return None;
                }

                let next_level_pgtbl: *mut PageTable = KMEM.kalloc();
                if next_level_pgtbl.is_null() {
                    return None;
                }

                memset(next_level_pgtbl as *mut u8, 0, PGSIZE);

                *pte = Pte(PA2PTE!(next_level_pgtbl.expose_addr()) | PTE_V);
                // printf!("[{}] pte: {:x}\n", PX!(level, va), pte.0);
                curr_pgtbl = next_level_pgtbl.as_mut().unwrap();
            }
        }
    }

    Some(&mut (curr_pgtbl.0)[PX!(0, va)])
}

// Look up a virtual address, return the physical address,
// or 0 if not mapped.
// Can only be used to look up user pages.
fn walkaddr(page_table: &mut PageTable, va: usize) -> Option<usize> {
    if va >= MAXVA {
        return None;
    }

    let pte = walk(page_table, va, 0);
    if pte.is_none() {
        return None;
    }

    let pte = pte.unwrap();
    if (pte.0 & PTE_V) == 0 {
        return None;
    }
    if (pte.0 & PTE_U) == 0 {
        return None
    }

    let pa = PTE2PA!(pte.0);
    return Some(pa);
}


// Switch h/w page table register to the kernel's page table,
// and enable paging.
pub fn kvminithart() {
    // wait for any previous writes to the page table memory to finish.
    sfence_vma();

    let addr = unsafe { (KERNEL_PAGETABLE.unwrap() as *const PageTable).expose_addr() };
    let satp = MAKE_SATP!(addr);
    w_satp(satp);

    // flush stale entries from the TLB.
    sfence_vma();
}

// create an empty user page table.
// returns 0 if out of memory.
pub fn uvmcreate<'a>() -> Option<&'a mut PageTable>{
    unsafe {
        let pagetable: *mut PageTable = KMEM.kalloc();
        if pagetable.is_null() {
            return None;
        }
        memset(pagetable as *mut u8, 0, PGSIZE);
        pagetable.as_mut()
    }
}

// Load the user initcode into address 0 of pagetable,
// for the very first process.
// sz must be less than a page.
pub fn uvmfirst(pagetable: &mut PageTable, src: *const u8, sz: usize) {
    if sz >= PGSIZE {
        panic!("uvmfirst: more than a page");
    }

    let mem = unsafe { KMEM.kalloc() };
    memset(mem, 0, PGSIZE);
    mappages(pagetable, 0, mem.expose_addr(), PGSIZE, PTE_W | PTE_R | PTE_X | PTE_U);
    memmove(mem, src, sz);
}

// Recursively free page-table pages.
// All leaf mappings must already have been removed.
fn freewalk(pagetable: &mut PageTable) {
    // there are 2^9 = 512 PTEs in a page table.
    for pte in &mut pagetable.0 {
        if pte.0 & PTE_V == 0 {
            panic!("freewalk: leaf");
        }

        if (pte.0 & PTE_V) ==0 && pte.0 & (PTE_R|PTE_W|PTE_X) == 0 {
            // this PTE points to a lower-level page table.
            let child_pgtbl = unsafe { (PTE2PA!(pte.0) as *mut PageTable).as_mut().unwrap() };
            freewalk(child_pgtbl);
            *pte = Pte(0);
        }
    }

    unsafe { KMEM.kfree(pagetable) };
}

// Free user memory pages,
// then free page-table pages.
pub fn uvmfree(pagetable: &mut PageTable, sz: usize) {
    if sz > 0 {
        uvmunmap(pagetable, 0, PGROUNDUP!(sz)/PGSIZE, true);
    }
    freewalk(pagetable);
}

// Allocate PTEs and physical memory to grow process from oldsz to
// newsz, which need not be page aligned.  Returns new size or 0 on error.
pub fn uvmalloc(page_table: &mut PageTable, oldsz: usize, newsz: usize, xperm: usize) -> usize {
    if newsz < oldsz {
        return oldsz;
    }

    let mut mem;
    let oldsz = PGROUNDUP!(oldsz);
    for a in (0..newsz).step_by(PGSIZE) {
        mem = unsafe { KMEM.kalloc() };
        if mem.is_null() {
            uvmdealloc(page_table, a, oldsz);
            return 0;
        }
        memset(mem, 0, PGSIZE);
        if mappages(page_table, a, PGSIZE, mem.expose_addr(), PTE_R|PTE_U|xperm) != 0 {
            unsafe { KMEM.kfree(mem); }
        }
        uvmdealloc(page_table, a, oldsz);
        return 0;
    }

    return newsz;
}

// Deallocate user pages to bring the process size from oldsz to
// newsz.  oldsz and newsz need not be page-aligned, nor does newsz
// need to be less than oldsz.  oldsz can be larger than the actual
// process size.  Returns the new process size.
pub fn uvmdealloc(page_table: &mut PageTable, oldsz: usize, newsz: usize) -> usize {
    if newsz >= oldsz {
        return oldsz;
    }

    if PGROUNDUP!(newsz) < PGROUNDUP!(oldsz) {
        let npages = (PGROUNDUP!(oldsz) - PGROUNDUP!(newsz)) / PGSIZE;
        uvmunmap(page_table, PGROUNDUP!(newsz), npages, true);
    }

    return newsz;
}

// Copy from user to kernel.
// Copy len bytes to dst from virtual address srcva in a given page table.
// Return 0 on success, -1 on error.
pub fn copyin(page_table: &mut PageTable, dst: *mut u8, srcva: usize, len: usize) -> i8 {
    let mut n = 0;
    let mut va0 = 0;
    let mut pa0 = 0;
    let mut srcva = srcva;
    let mut len = len;

    while len > 0 {
        va0 = PGROUNDDOWN!(srcva);
        let pa0_op = walkaddr(page_table, va0);
        if pa0_op.is_none() {
            return -1;
        }
        pa0 = pa0_op.unwrap();

        n = PGSIZE - (srcva - va0);
        if n > len {
            n = len;
        }
        memmove(dst, (pa0 + (srcva - va0)) as *mut u8, n);

        len -= n;
        unsafe { dst.add(n) };
        srcva = va0 + PGSIZE;
    }
    return 0;
}

// Copy a null-terminated string from user to kernel.
// Copy bytes to dst from virtual address srcva in a given page table,
// until a '\0', or max.
// Return 0 on success, -1 on error.
pub fn copyinstr(page_table: &mut PageTable, dst: *mut char, srcva: usize, max: usize) -> i8 {
    let mut n = 0;
    let mut va0 = 0;
    let mut pa0 = 0;
    let mut got_null = 0;
    let mut srcva = srcva;
    let mut max = max;

    while got_null == 0 && max > 0 {
        va0 = PGROUNDDOWN!(srcva);
        let pa0_op = walkaddr(page_table, va0);
        if pa0_op.is_none() {
            return -1;
        }
        pa0 = pa0_op.unwrap();

        n = PGSIZE - (srcva - va0);
        if n > max {
            n = max;
        }

        let p = (pa0 + (srcva - va0)) as *mut char;
        unsafe {
            while n > 0 {
                if *p == '\0' {
                    *dst = '\0';
                    got_null = 1;
                    break;
                } else {
                    *dst = *p;
                }
                n -= 1;
                max -= 1;
                let _ = p.add(1);
                let _ = dst.add(1);
            }
        }

        srcva = va0 + PGSIZE;
    }

    if got_null {
        return 0;
    } else {
        return -1;
    }
}
