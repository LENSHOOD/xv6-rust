use core::mem;
use crate::exec::exec;
use crate::file::fcntl::{O_CREATE, O_RDONLY};
use crate::file::INode;
use crate::fs::fs::namei;
use crate::kalloc::KMEM;
use crate::log::{begin_op, end_op};
use crate::param::{MAXARG, MAXPATH, NDEV};
use crate::riscv::PGSIZE;
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

        if(fetchaddr(uargv+mem::size_of::<usize>()*i, &mut uarg) < 0){
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

fn sys_open() -> u64 {
    char path[MAXPATH];
    int fd, omode;
    struct file *f;
    struct inode *ip;
    int n;

    let omode = argint(1);
    let n = argstr(0, path, MAXPATH)
    if n < 0 {
        return -1;
    }

    begin_op();

    if omode & O_CREATE {
        ip = create(path, T_FILE, 0, 0);
        if ip == 0 {
            end_op();
            return -1;
        }
    } else {
        if (ip = namei(path)) == 0 {
            end_op();
            return -1;
        }
        ilock(ip);
        if ip ->type == T_DIR && omode != O_RDONLY){
            iunlockput(ip);
            end_op();
            return -1;
        }
    }

    if ip ->type == T_DEVICE && (ip->major < 0 || ip->major >= NDEV)){
        iunlockput(ip);
        end_op();
        return -1;
    }

    if (f = filealloc()) == 0 || (fd = fdalloc(f)) < 0 {
        if f {
            fileclose(f);
        }
        iunlockput(ip);
        end_op();
        return -1;
    }

    if ip ->type == T_DEVICE){
        f->type = FD_DEVICE;
        f->major = ip->major;
    } else {
        f->type = FD_INODE;
        f->off = 0;
    }
    f->ip = ip;
    f->readable = !(omode & O_WRONLY);
    f->writable = (omode & O_WRONLY) || (omode & O_RDWR);

    if (omode & O_TRUNC) && ip ->type == T_FILE){
        itrunc(ip);
    }

    iunlock(ip);
    end_op();

    return fd;
}

fn create(char *path, short type, short major, short minor) -> &INode {
    struct inode *ip, *dp;
    char name[DIRSIZ];

    if((dp = nameiparent(path, name)) == 0)
    return 0;

    ilock(dp);

    if((ip = dirlookup(dp, name, 0)) != 0){
    iunlockput(dp);
    ilock(ip);
    if(type == T_FILE && (ip->type == T_FILE || ip->type == T_DEVICE))
    return ip;
    iunlockput(ip);
    return 0;
    }

    if((ip = ialloc(dp->dev, type)) == 0){
    iunlockput(dp);
    return 0;
    }

    ilock(ip);
    ip->major = major;
    ip->minor = minor;
    ip->nlink = 1;
    iupdate(ip);

    if(type == T_DIR){  // Create . and .. entries.
    // No ip->nlink++ for ".": avoid cyclic ref count.
    if(dirlink(ip, ".", ip->inum) < 0 || dirlink(ip, "..", dp->inum) < 0)
    goto fail;
    }

    if(dirlink(dp, name, ip->inum) < 0)
    goto fail;

    if(type == T_DIR){
    // now that success is guaranteed:
    dp->nlink++;  // for ".."
    iupdate(dp);
    }

    iunlockput(dp);

    return ip;

    fail:
    // something went wrong. de-allocate ip.
    ip->nlink = 0;
    iupdate(ip);
    iunlockput(ip);
    iunlockput(dp);
    return 0;
}
