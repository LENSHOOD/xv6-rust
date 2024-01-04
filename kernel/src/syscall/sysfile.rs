use core::mem;
use crate::exec::exec;
use crate::kalloc::KMEM;
use crate::NELEM;
use crate::param::{MAXARG, MAXPATH};
use crate::riscv::PGSIZE;
use crate::syscall::syscall::{argaddr, argstr, fetchaddr, fetchstr};

fn sys_exec() -> u64 {
    let mut uarg: usize = 0;
    let uargv = argaddr(1);

    let mut path: [u8; MAXPATH] = ['\0' as u8; MAXPATH];
    if(argstr(0, &mut path, MAXPATH) < 0) {
        return u64::MAX;
    }

    let mut argv: [Option<&mut [u8]>; MAXARG] = [None; MAXARG];
    let mut i = 0;
    let mut bad = false;
    loop {
        if i >= NELEM!(argv) {
            bad = true;
            break
        }

        if(fetchaddr(uargv+mem::size_of::<usize>()*i, &mut uarg) < 0){
            bad = true;
            break
        }

        if uarg == 0 {
            argv[i] = None;
            break;
        }

        argv[i] = unsafe { Some(KMEM.kalloc() as &mut [u8]) };
        if argv[i] == None {
            bad = true;
            break
        }

        if fetchstr(uarg, argv[i].unwrap(), PGSIZE) < 0 {
            bad = true;
            break
        }

        i += 1;
    }

    let mut ret = -1;
    if !bad {
        ret = exec(&path, &argv);
    }

    for i in 0..argv {
        if argv[i].is_none() {
            break
        }

        unsafe { KMEM.kfree(argv[i].unwrap()) }
    }

    return ret as u64;
}
