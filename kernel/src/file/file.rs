use crate::file::FDType::{FD_DEVICE, FD_INODE, FD_NONE, FD_PIPE};
use crate::file::File;
use crate::log::{begin_op, end_op};
use crate::param::NFILE;
use crate::spinlock::Spinlock;

struct FTable<'a> {
    lock: Spinlock,
    file: [File<'a>; NFILE]
}

static mut FTABLE: FTable = FTable {
    lock: Spinlock::init_lock("ftable"),
    file: [File::create(); NFILE],
};

pub fn fileinit() {
    // empty due to FTABLE has already been initialized
}

// Allocate a file structure.
pub fn filealloc<'a>() -> Option<&'a mut File> {
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
            pipeclose(pipe, writable);
        } else if file_type == FD_INODE || file_type == FD_DEVICE {
            begin_op();
            ip.unwrap().iput();
            end_op();
        }
    }
}
