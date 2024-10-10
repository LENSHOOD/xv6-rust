use crate::file::file::filedup;
use crate::param::NOFILE;
use crate::proc::{allocproc, fork, freeproc, growproc, myproc, wait};
use crate::proc::{exit, Procstate::RUNNABLE, WAIT_LOCK};
use crate::syscall::syscall::{argaddr, argint};
use crate::vm::uvmcopy;

pub(crate) fn sys_exit() -> u64 {
    let n = argint(0);
    exit(n);
    return 0; // not reached
}

pub(crate) fn sys_fork() -> u64 {
    return fork().unwrap_or_else(|| u32::MAX) as u64;
}

pub(crate) fn sys_sbrk() -> u64 {
    let n = argint(0);
    let addr = myproc().sz;
    if growproc(n) < 0 {
        return -1i64 as u64;
    }
    return addr as u64;
}

pub(crate) fn sys_wait() -> u64 {
    let p = argaddr(0);
    return wait(p) as u64;
}
