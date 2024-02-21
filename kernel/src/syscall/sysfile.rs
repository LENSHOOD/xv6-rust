use core::mem;
use crate::exec::exec;
use crate::file::fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use crate::file::file::{filealloc, fileclose};
use crate::file::{File, INode};
use crate::file::FDType::{FD_DEVICE, FD_INODE};
use crate::fs::fs::{dirlink, dirlookup, ialloc, namei, nameiparent};
use crate::kalloc::KMEM;
use crate::log::{begin_op, end_op};
use crate::param::{MAXARG, MAXPATH, NDEV, NOFILE};
use crate::proc::myproc;
use crate::riscv::PGSIZE;
use crate::stat::FileType;
use crate::stat::FileType::{T_DEVICE, T_DIR, T_FILE};
use crate::syscall::syscall::{argaddr, argint, argstr, fetchaddr, fetchstr};

fn sys_exec() -> u64 {
    let mut uarg: usize = 0;
    let uargv = argaddr(1);

    let mut path: [u8; MAXPATH] = ['\0' as u8; MAXPATH];
    if argstr(0, &mut path as *mut u8, MAXPATH) < 0 {
        return u64::MAX;
    }

    let mut argv: [Option<*mut u8>; MAXARG] = [None; MAXARG];
    let mut i = 0;
    let mut bad = false;
    loop {
        if i >= argv.len() {
            bad = true;
            break
        }

        if fetchaddr(uargv+mem::size_of::<usize>()*i, &mut uarg) < 0 {
            bad = true;
            break
        }

        if uarg == 0 {
            argv[i] = None;
            break;
        }

        let ptr: *mut u8 = unsafe { KMEM.kalloc() };
        if ptr.is_null() {
            bad = true;
            break
        }
        argv[i] = Some(ptr);


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

    for i in 0..argv.len() {
        if argv[i].is_none() {
            break
        }

        unsafe { KMEM.kfree(argv[i].unwrap()) }
    }

    return ret as u64;
}

fn sys_open() -> Option<usize> {
    let mut path: [u8; MAXPATH] = ['\0' as u8; MAXPATH];
    let omode = argint(1);
    let n = argstr(0, &mut path as *mut u8, MAXPATH);
    if n < 0 {
        return None;
    }

    begin_op();

    let mut ip = None;
    let path = unsafe { core::str::from_utf8_unchecked(&path) };
    if omode & O_CREATE != 0 {
        ip = create(path, T_FILE, 0, 0);
        if ip.is_none() {
            end_op();
            return None;
        }
    } else {
        ip = namei(path);
        if ip.is_none() {
            end_op();
            return None;
        }

        let ip = ip.as_mut()?;
        ip.ilock();
        if ip.file_type == T_DIR && omode != O_RDONLY {
            ip.iunlockput();
            end_op();
            return None;
        }
    }

    let ip = ip?;
    if ip.file_type == T_DEVICE && (ip.major < 0 || ip.major as usize >= NDEV) {
        ip.iunlockput();
        end_op();
        return None;
    }


    let f = filealloc();
    if f.is_none() {
        ip.iunlockput();
        end_op();
        return None;
    }

    let f = f?;
    let fd = fdalloc(f);
    if fd.is_none() {
        fileclose(f);
        ip.iunlockput();
        end_op();
        return None;
    }

    if ip.file_type == T_DEVICE {
        f.file_type = FD_DEVICE;
        f.major = ip.major;
    } else {
        f.file_type = FD_INODE;
        f.off = 0;
    }
    f.ip = Some(ip);
    f.readable = omode & O_WRONLY == 0;
    f.writable = (omode & O_WRONLY) != 0 || (omode & O_RDWR) != 0;

    if (omode & O_TRUNC) != 0 && ip.file_type == T_FILE {
        ip.itrunc();
    }

    ip.iunlock();
    end_op();

    return fd;
}

pub fn sys_mknod() -> i64 {
    begin_op();
    let major = argint(1)  as i16;
    let minor = argint(2)  as i16;

    let mut path = [0; MAXPATH];

    if (argstr(0, &mut path as *mut u8, MAXPATH)) < 0 {
        end_op();
        return -1;
    }

    let path_str = unsafe { core::str::from_utf8_unchecked(&path) };
    let ip = create(path_str, T_DEVICE, major, minor);
    if ip.is_none() {
        end_op();
        return -1;
    }

    ip.unwrap().iunlockput();
    end_op();
    return 0;
}


fn create<'a>(path: &str, file_type: FileType, major: i16, minor: i16) -> Option<&'a mut INode> {
    let dp = nameiparent(path)?;
    dp.ilock();

    let ip = dirlookup(dp, "", &mut 0);
    if ip.is_some() {
        let ip = ip?;
        dp.iunlockput();
        ip.ilock();
        if file_type == T_FILE && (ip.file_type == T_FILE || ip.file_type == T_DEVICE) {
            return Some(ip);
        }
        ip.iunlockput();
        return None;
    }

    let ip = ialloc(dp.dev, file_type);
    if ip.is_none() {
        dp.iunlockput();
        return None;
    }

    let ip = ip?;
    ip.ilock();
    ip.major = major;
    ip.minor = minor;
    ip.nlink = 1;
    ip.iupdate();

    if file_type == T_DIR {  // Create . and .. entries.
        // No ip->nlink++ for ".": avoid cyclic ref count.
        if dirlink(ip, ".", ip.inum as u16).is_none() || dirlink(ip, "..", dp.inum as u16).is_none() {
            // something went wrong. de-allocate ip.
            ip.nlink = 0;
            ip.iupdate();
            ip.iunlockput();
            dp.iunlockput();
            return None;
        }
    }

    if dirlink(dp, "", ip.inum as u16).is_none() {
        // something went wrong. de-allocate ip.
        ip.nlink = 0;
        ip.iupdate();
        ip.iunlockput();
        dp.iunlockput();
        return None;
    }

    if file_type == T_DIR {
        // now that success is guaranteed:
        dp.nlink += 1;  // for ".."
        ip.iupdate();
    }

    dp.iunlockput();

    return Some(ip);
}

// Allocate a file descriptor for the given file.
// Takes over file reference from caller on success.
fn fdalloc(f: *mut File) -> Option<usize> {
    let p = myproc();

    for fd in 0..NOFILE {
        if p.ofile[fd].is_none() {
            p.ofile[fd] = Some(f);
            return Some(fd);
        }
    }

    return None;
}
