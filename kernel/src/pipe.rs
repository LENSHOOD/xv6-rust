use crate::kalloc::KMEM;
use crate::proc::{killed, myproc, sleep, wakeup};
use crate::spinlock::Spinlock;
use crate::vm::copyin;

const PIPESIZE: usize = 512;
pub struct Pipe {
    lock: Spinlock,
    data: [u8; PIPESIZE],
    nread: u32,      // number of bytes read
    nwrite: u32,     // number of bytes written
    readopen: bool,  // read fd is still open
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
            unsafe {
                KMEM.kfree(self as *mut Pipe);
            }
        } else {
            self.lock.release();
        }
    }

    pub(crate) fn write(self: &mut Self, addr: usize, n: i32) -> i32 {
        let pr = myproc();

        self.lock.acquire();

        let mut i = 0;
        while i < n {
            if !self.readopen || killed(pr) != 0 {
                self.lock.release();
                return -1;
            }

            if self.nwrite == self.nread + PIPESIZE as u32 {
                //DOC: pipewrite-full
                wakeup(&self.nread);
                sleep(&self.nwrite, &mut self.lock);
            } else {
                let mut ch = 0;
                let pgtbl = unsafe { pr.pagetable.unwrap().as_mut().unwrap() };
                if copyin(pgtbl, &mut ch as *mut u8, addr + i as usize, 1) == -1 {
                    break;
                }
                self.data[self.nwrite as usize % PIPESIZE] = ch;
                self.nwrite += 1;
                i += 1;
            }
        }
        wakeup(&self.nread);
        self.lock.release();
        return i;
    }
}
