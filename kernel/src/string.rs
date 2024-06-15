use crate::riscv::PGSIZE;

pub fn memset(dst: *mut u8, c: u8, n: usize) -> *mut u8 {
    unsafe { dst.write_bytes(c, n) }
    dst
}

pub fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    unsafe {
        src.copy_to(dst, n);
    }
    dst
}

pub fn strlen(s: *const u8) -> usize {
    for i in 0..PGSIZE {
        unsafe {
            if *s.add(i) == '\0' as u8 {
                return i;
            }
        }
    }

    panic!("too long slice")
}
