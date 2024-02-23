use core::mem;
use crate::file::file::filedup;
use crate::param::NOFILE;
use crate::proc::{allocproc, freeproc, myproc, Trapframe};

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
    if uvmcopy(p.pagetable, np.pagetable, p.sz) < 0 {
        freeproc(np);
        &np.lock.release();
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
    unsafe { np.trapframe.unwrap().as_mut().unwrap().a0 = 0; }

    // increment reference counts on open file descriptors.
    for i in 0..NOFILE {
        if p.ofile[i].is_some() {
            let f = p.ofile[i].unwrap();
            filedup(f);
            np.ofile[i] = Some(f);
        }
    }
    np.cwd = idup(p.cwd);

    safestrcpy(np.name, p.name, mem::size_of_val(&p.name));

    let pid = np.pid;

    np.lock.release();

    WAIT_LOCK.acquire();
    np.parent = Some(p);
    WAIT_LOCK.release();

    np.lock.acquire();
    np.state = RUNNABLE;
    np.lock.release();

    return Some(pid);
}
