pub fn memset(dst: *mut u8, c: u8, n: usize) -> *mut u8{
    for i in 0..n {
        unsafe {
            dst.add(i).write(c)
        }
    }
    dst
}