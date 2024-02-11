use crate::kalloc::KMEM;
use crate::proc::wakeup;
use crate::spinlock::Spinlock;

const PIPESIZE: usize = 512;
pub struct Pipe {
    lock: Spinlock,
    data: [u8; PIPESIZE],
    nread: u32, // number of bytes read
    nwrite: u32, // number of bytes written
    readopen: bool, // read fd is still open
    writeopen: bool, // write fd is still open
}

impl Pipe {
    pub(crate) fn close(self: &mut Self, writable: bool) {
        self.lock.acquire();
        if writable {
            self.writeopen = false;
            wakeup(&self.nread);
        } else {
            self.readopen = false;
            wakeup(&self.nwrite);
        }
        if !self.readopen && !self.writeopen {
            self.lock.release();
            unsafe { KMEM.kfree(self as *mut Pipe); }
        } else {
            self.lock.release();
        }
    }
}