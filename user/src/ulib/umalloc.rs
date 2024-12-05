// Memory allocator by Kernighan and Ritchie,
// The C programming Language, 2nd ed.  Section 8.7.

use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ptr::null_mut;

use crate::stubs::sbrk;

#[global_allocator]
static ALLOCATOR: UserAllocator = UserAllocator {};

struct UserAllocator {}
unsafe impl Sync for UserAllocator {}
unsafe impl GlobalAlloc for UserAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        malloc(layout.size() as u32)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        free(ptr)
    }
}

type Align = u64;

#[derive(Clone, Copy)]
struct S {
    ptr: *mut Header,
    size: u32,
}

union Header {
    s: S,
    x: Align,
}

static mut BASE: Header = Header {
    s: S {
        ptr: null_mut(),
        size: 0,
    },
};
static mut FREEP: *mut Header = null_mut();

unsafe fn free(ap: *mut u8) {
    let bp = (ap as *mut Header).sub(1);

    let mut p = FREEP;
    loop {
        if bp > p && bp < (*p).s.ptr {
            break;
        }

        if p >= (*p).s.ptr && (bp > p || bp < (*p).s.ptr) {
            break;
        }

        p = (*p).s.ptr;
    }

    if bp.addr() + (*bp).s.size as usize == (*p).s.ptr.addr() {
        (*bp).s.size += (*(*p).s.ptr).s.size;
        (*bp).s.ptr = (*(*p).s.ptr).s.ptr;
    } else {
        (*bp).s.ptr = (*p).s.ptr;
    }

    if p.addr() + (*p).s.size as usize == bp.addr() {
        (*p).s.size += (*bp).s.size;
        (*p).s.ptr = (*bp).s.ptr;
    } else {
        (*p).s.ptr = bp;
    }

    FREEP = p;
}

unsafe fn morecore(nu: u32) -> *mut Header {
    let mut nu = nu;
    if nu < 4096 {
        nu = 4096;
    }
    let p: *mut u8 = sbrk(nu * mem::size_of::<Header>() as u32);
    if p.is_null() {
        return null_mut();
    }

    let hp = p as *mut Header;
    (*hp).s.size = nu;
    free(hp.add(1) as *mut u8);
    return FREEP;
}

unsafe fn malloc(nbytes: u32) -> *mut u8 {
    let sz = mem::size_of::<Header>() as u32;
    let nunits = (nbytes + sz - 1) / sz + 1;

    let mut prevp = FREEP;
    if prevp.is_null() {
        prevp = &mut BASE as * mut Header;
        FREEP = prevp;
        BASE.s.ptr = FREEP;
        BASE.s.size = 0;
    }

    let mut p = (*prevp).s.ptr;
    loop {
        if (*p).s.size >= nunits {
            if (*p).s.size == nunits {
                (*prevp).s.ptr = (*p).s.ptr;
            } else {
                (*p).s.size -= nunits;
                p = p.add((*p).s.size as usize);
                (*p).s.size = nunits;
            }
            FREEP = prevp;
            return p.add(1) as *mut u8;
        }
        if p == FREEP {
            p = morecore(nunits);
            if p.is_null() {
                return null_mut();
            }
        }

        prevp = p;
        p = (*p).s.ptr;
    }
}
