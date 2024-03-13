use core::mem;
use crate::printf;
use crate::proc::myproc;
use crate::string::strlen;
use crate::syscall::{SYS_chdir, SYS_close, SYS_dup, SYS_exec, SYS_exit, SYS_fork, SYS_fstat, SYS_getpid, SYS_kill, SYS_link, SYS_mkdir, SYS_mknod, SYS_open, SYS_pipe, SYS_read, SYS_sbrk, SYS_sleep, SYS_unlink, SYS_uptime, SYS_wait, SYS_write};
use crate::syscall::sysfile::{sys_dup, sys_mknod, sys_write};
use crate::syscall::sysproc::{sys_exit, sys_fork, sys_wait};
use crate::vm::{copyin, copyinstr};

// Retrieve an argument as a pointer.
// Doesn't check for legality, since
// copyin/copyout will do that.
pub(super) fn argaddr(n: u8) -> usize {
    argraw(n) as usize
}

// Fetch the nth 32-bit system call argument.
pub(super) fn argint(n: u8) -> i32 {
    return argraw(n) as i32;
}

// Fetch the nth word-sized system call argument as a null-terminated string.
// Copies into buf, at most max.
// Returns string length if OK (including nul), -1 if error.
pub(super) fn argstr(n: u8, buf: *mut u8, max: usize) -> i32 {
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
pub(super) fn fetchstr(addr: usize, buf: *mut u8, max: usize) -> i32 {
    let p = myproc();
    if unsafe { copyinstr(p.pagetable.unwrap().as_mut().unwrap(), buf, addr, max) } < 0 {
        return -1;
    }
    return strlen(buf) as i32;
}

// An array mapping syscall numbers from syscall.h
// to the function that handles the system call.
static SYSCALL: [(u8, fn() -> usize); 4] = [
    (SYS_fork, sys_fork),
    (SYS_exit, sys_exit),
    (SYS_wait, sys_wait),
    (SYS_pipe, sys_fork),
    // (SYS_read, ),
    // (SYS_kill, ),
    // (SYS_exec, ),
    // (SYS_fstat, ),
    // (SYS_chdir, ),
    // (SYS_dup, sys_dup),
    // (SYS_getpid, ),
    // (SYS_sbrk, ),
    // (SYS_sleep, ),
    // (SYS_uptime, ),
    // (SYS_open, ),
    // (SYS_write, sys_write),
    // (SYS_mknod, sys_mknod),
    // (SYS_unlink, ),
    // (SYS_link, ),
    // (SYS_mkdir, ),
    // (SYS_close, ),
];

pub fn syscall() {
    let p = myproc();

    let tf = unsafe { p.trapframe.unwrap().as_mut().unwrap() };
    let num = tf.a7 as usize;

    if num > 0 && num < SYSCALL.len() {
        // Use num to lookup the system call function for num, call it,
        // and store its return value in p->trapframe->a0
        tf.a0 = SYSCALL[num].1 as u64;
    } else {
        printf!("{} {}: unknown sys call {}\n", p.pid, core::str::from_utf8(&p.name).unwrap(), num);
        tf.a0 = u64::MAX;
    }
}
