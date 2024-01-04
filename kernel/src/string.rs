pub fn memset(dst: *mut u8, c: u8, n: usize) -> *mut u8{
    for i in 0..n {
        unsafe {
            dst.add(i).write(c)
        }
    }
    dst
}

pub fn memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    unsafe { src.copy_to(dst, n); }
    dst
}

pub fn strlen(s: &[u8]) -> usize {
    for i in 0..s.len() {
        if s[i] == '\0' as u8 {
            return i;
        }
    }

    return s.len()
}
