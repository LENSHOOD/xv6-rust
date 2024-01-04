use core::mem;
use crate::proc::myproc;
use crate::string::strlen;
use crate::vm::{copyin, copyinstr};

// Retrieve an argument as a pointer.
// Doesn't check for legality, since
// copyin/copyout will do that.
pub(super) fn argaddr(n: u8) -> usize {
    argraw(n) as usize
}

// Fetch the nth word-sized system call argument as a null-terminated string.
// Copies into buf, at most max.
// Returns string length if OK (including nul), -1 if error.
pub(super) fn argstr(n: u8, buf: &mut [u8], max: usize) -> i32 {
    let addr = argaddr(n);
    return fetchstr(addr, buf, max);
}

fn argraw(n: u8) -> u64 {
    let p = myproc();
    let tf = unsafe { p.trapframe.unwrap().as_ref() }.unwrap();
    match n {
        0 => tf.a0,
        1 => tf.a1,
        2 => tf.a2,
        3 => tf.a3,
        4 => tf.a4,
        5 => tf.a5,
        _ => {
            panic!("argraw")
        }
    }
}

// Fetch the uint64 at addr from the current process.
pub(super) fn fetchaddr(addr: usize, ip: &mut usize) -> i32 {
    let p = myproc();
    if addr >= p.sz || addr+mem::size_of::<usize>() > p.sz { // both tests needed, in case of overflow
        return -1;
    }
    if unsafe { copyin(p.pagetable.unwrap().as_mut().unwrap(), ip as *mut usize as *mut u8, addr, mem::size_of::<usize>())} != 0 {
        return -1;
    }
    return 0;
}


// Fetch the nul-terminated string at addr from the current process.
// Returns length of string, not including nul, or -1 for error.
pub(super) fn fetchstr(addr: usize, buf: &mut [u8], max: usize) -> i32 {
    let p = myproc();
    if unsafe { copyinstr(p.pagetable.unwrap().as_mut().unwrap(), buf.as_mut_ptr(), addr, max) } < 0 {
        return -1;
    }
    return strlen(buf) as i32;
}