use crate::file::FDType::{FD_DEVICE, FD_INODE, FD_NONE, FD_PIPE};
use crate::file::{File, DEVSW};
use crate::fs::BSIZE;
use crate::log::{begin_op, end_op};
use crate::param::{MAXOPBLOCKS, NDEV, NFILE};
use crate::spinlock::Spinlock;

struct FTable {
    lock: Spinlock,
    file: [File; NFILE],
}

static mut FTABLE: FTable = FTable {
    lock: Spinlock::init_lock("ftable"),
    file: [File::create(); NFILE],
};

pub fn fileinit() {
    // empty due to FTABLE has already been initialized
}

// Allocate a file structure.
pub fn filealloc() -> Option<&'static mut File> {
    unsafe {
        FTABLE.lock.acquire();
        for f in &mut FTABLE.file {
            if f.ref_cnt == 0 {
                f.ref_cnt = 1;
                FTABLE.lock.release();
                return Some(f);
            }
        }

        FTABLE.lock.release();
        return None;
    }
}

// Increment ref count for file f.
pub(crate) fn filedup(f: *mut File) {
    unsafe {
        FTABLE.lock.acquire();
        let f = f.as_mut().unwrap();
        if f.ref_cnt < 1 {
            panic!("filedup")
        }

        f.ref_cnt += 1;
        FTABLE.lock.release();
    }
}

// Close file f.  (Decrement ref count, close when reaches 0.)
pub(crate) fn fileclose(f: &mut File) {
    unsafe {
        FTABLE.lock.acquire();
        if f.ref_cnt < 1 {
            panic!("fileclose");
        }

        f.ref_cnt -= 1;
        if f.ref_cnt > 0 {
            FTABLE.lock.release();
            return;
        }

        let file_type = f.file_type;
        let pipe = f.pipe;
        let writable = f.writable;
        let ip = f.ip;

        f.ref_cnt = 0;
        f.file_type = FD_NONE;
        FTABLE.lock.release();

        if file_type == FD_PIPE {
            pipe.unwrap().as_mut().unwrap().close(writable);
        } else if file_type == FD_INODE || file_type == FD_DEVICE {
            begin_op();
            ip.unwrap().as_mut().unwrap().iput();
            end_op();
        }
    }
}

// Write to file f.
// addr is a user virtual address.
pub(crate) fn filewrite(f: &mut File, addr: usize, n: i32) -> i32 {
    if !f.writable {
        return -1;
    }

    match f.file_type {
        FD_PIPE => unsafe { f.pipe.unwrap().as_mut().unwrap().write(addr, n) },
        FD_DEVICE => {
            if f.major < 0
                || f.major as usize >= NDEV
                || unsafe { DEVSW[f.major as usize].is_none() }
            {
                return -1;
            }
            unsafe {
                DEVSW[f.major as usize]
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .write(true, addr, n as usize)
            }
        }
        FD_INODE => {
            // write a few blocks at a time to avoid exceeding
            // the maximum log transaction size, including
            // i-node, indirect block, allocation blocks,
            // and 2 blocks of slop for non-aligned writes.
            // this really belongs lower down, since writei()
            // might be writing a device like the console.
            let max = (((MAXOPBLOCKS - 1 - 1 - 2) / 2) * BSIZE) as i32;
            let mut i = 0;
            let mut r = 0;
            while i < n {
                let mut n1 = n - i;
                if n1 > max {
                    n1 = max;
                }

                begin_op();
                let ip = unsafe { f.ip.unwrap().as_mut().unwrap() };
                ip.ilock();
                r = ip.writei(true, (addr + i as usize) as *mut u8, f.off, n1 as usize) as i32;
                if r > 0 {
                    f.off += r as u32;
                }
                ip.iunlock();
                end_op();

                if r != n1 {
                    // error from writei
                    break;
                }
                i += r;
            }

            if i == n {
                n
            } else {
                -1
            }
        }
        FD_NONE => panic!("filewrite"),
    }
}
