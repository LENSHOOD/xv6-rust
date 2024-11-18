use core::ptr;

use crate::memlayout::PHYSTOP;
use crate::riscv::PGSIZE;
use crate::spinlock::Spinlock;
use crate::string::memset;
use crate::PGROUNDUP;

extern "C" {
    // first address after kernel.
    // defined by kernel.ld.
    static mut end: u8;
}

struct Run {
    next: *mut Run,
}

pub struct KMem {
    lock: Spinlock,
    freelist: *mut Run,
}

pub static mut KMEM: KMem = KMem::create();

impl KMem {
    const fn create() -> Self {
        Self {
            lock: Spinlock::init_lock("kmem"),
            freelist: ptr::null_mut(),
        }
    }
    pub fn kinit() {
        unsafe {
            KMEM.freerange((&mut end) as *mut u8, PHYSTOP as *mut u8);
        }

        // printf!("finish init from {:x}, to {:x}", unsafe { (&end as *const u8).expose_addr() }, PHYSTOP);
    }

    fn freerange<T: Sized>(self: &mut Self, pa_start: *mut T, pa_end: *mut T) {
        let mut p = PGROUNDUP!(pa_start);
        while p + PGSIZE <= pa_end as usize {
            self.kfree(p as *mut T);
            p += PGSIZE;
        }
    }

    /// Free the page of physical memory pointed at by pa,
    /// which normally should have been returned by a
    /// call to kalloc().  (The exception is when
    /// initializing the allocator; see kinit above.)
    pub fn kfree<T: Sized>(self: &mut Self, pa: *mut T) {
        unsafe {
            let pa_uszie = pa as usize;
            if pa_uszie % PGSIZE != 0
                || pa_uszie < ((&end) as *const u8) as usize
                || pa_uszie >= PHYSTOP
            {
                panic!("kfree");
            }
        }

        // Fill with junk to catch dangling refs.
        memset(pa as *mut u8, 1, PGSIZE);

        let r = pa as *mut Run;

        self.lock.acquire();
        unsafe {
            (*r).next = self.freelist;
        }
        self.freelist = r;
        self.lock.release();
    }

    /// Allocate one 4096-byte page of physical memory.
    /// Returns a pointer that the kernel can use.
    /// Returns 0 if the memory cannot be allocated.
    pub fn kalloc<T: Sized>(self: &mut Self) -> *mut T {
        self.lock.acquire();
        let r = self.freelist;
        if !r.is_null() {
            unsafe {
                self.freelist = (*r).next;
            }
        }
        self.lock.release();

        if !r.is_null() {
            memset(r as *mut u8, 5, PGSIZE); // fill with junk
        }
        r as *mut T
    }
}
