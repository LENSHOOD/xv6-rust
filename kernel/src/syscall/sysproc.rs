use core::mem;
use crate::file::file::filedup;
use crate::param::NOFILE;
use crate::proc::{allocproc, freeproc, myproc, Trapframe};
use crate::vm::uvmcopy;
use crate::proc::{Procstate::RUNNABLE, WAIT_LOCK, exit};
use crate::syscall::syscall::argint;

pub(crate) fn sys_exit() -> usize {
    let n = argint(0);
    exit(n);
    return 0;  // not reached
}


pub(crate) fn sys_fork() -> u64 {
    return match fork() {
        Some(pid) => pid,
        None => u32::MAX
    } as u64
}

// Create a new process, copying the parent.
// Sets up child kernel stack to return as if from fork() system call.
fn fork() -> Option<u32> {
    let p = myproc();

    // Allocate process.
    let np = allocproc()?;

    // Copy user memory from parent to child.
    if unsafe { uvmcopy(p.pagetable?.as_mut()?, np.pagetable?.as_mut()?, p.sz) } < 0 {
        freeproc(np);
        let _ = &np.lock.release();
        return None;
    }
    np.sz = p.sz;

    // copy saved user registers.
    p.trapframe.map(|t| {
        let sz = mem::size_of::<Trapframe>();
        let dest = np.trapframe.unwrap();
        unsafe { t.copy_to(dest, sz); }
    });

    // Cause fork to return 0 in the child.
    unsafe { np.trapframe?.as_mut()?.a0 = 0; }

    // increment reference counts on open file descriptors.
    for i in 0..NOFILE {
        if p.ofile[i].is_some() {
            let f = p.ofile[i]?;
            filedup(f);
            np.ofile[i] = Some(f);
        }
    }

    unsafe { p.cwd?.as_mut()?.idup() };
    np.cwd = p.cwd;

    np.name.copy_from_slice(&p.name);

    let pid = np.pid;

    np.lock.release();

    unsafe {
        WAIT_LOCK.acquire();
        np.parent = Some(p);
        WAIT_LOCK.release();
    }

    np.lock.acquire();
    np.state = RUNNABLE;
    np.lock.release();

    return Some(pid);
}
