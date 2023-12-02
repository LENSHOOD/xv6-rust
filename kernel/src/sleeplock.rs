use crate::proc::{myproc, sleep, wakeup};
use crate::spinlock::Spinlock;

// Long-term locks for processes
#[derive(Copy, Clone)]
pub struct Sleeplock {
    locked: u64,       // Is the lock held?
    lk: Spinlock, // spinlock protecting this sleep lock

    // For debugging:
    name: &'static str,        // Name of lock.
    pid: u32           // Process holding lock
}

impl Sleeplock {
    pub const fn init_lock(name: &'static str) -> Self {
        Sleeplock {
            locked: 0,
            lk: Spinlock::init_lock("sleep lock"),
            name,
            pid: 0,
        }
    }

    pub fn acquire_sleep(self: &mut Self) {
        self.lk.acquire();

        while self.locked != 0 {
            sleep(self as *const Sleeplock, &mut self.lk);
        }
        self.locked = 1;
        let p = myproc();
        self.pid = p.pid;
        self.lk.release();
    }

    pub fn release_sleep(self: &mut Self) {
        self.lk.acquire();
        self.locked = 0;
        self.pid = 0;
        wakeup(&self);
        self.lk.release();
    }

    pub fn holding_sleep(self: &mut Self) -> bool {
        self.lk.acquire();
        let p = myproc();
        let r = self.locked != 0 && (self.pid == p.pid);
        self.lk.release();
        return r;
    }
}