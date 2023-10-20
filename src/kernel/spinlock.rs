use crate::proc::{Cpu, mycpu};
use crate::riscv::{__sync_lock_release, __sync_lock_test_and_set, __sync_synchronize, intr_get, intr_off, intr_on};

#[derive(Copy, Clone)]
pub struct Spinlock {
    locked: u64,             // Is the lock held?

    // For debugging:
    name: &'static str,      // Name of lock.
    cpu: Option<*mut Cpu<'static>>,   // The cpu holding the lock.
}

impl Spinlock {
    pub const fn init_lock(name: &'static str) -> Self {
        Spinlock {
            locked: 0,
            name,
            cpu: None,
        }
    }

    /// Acquire the lock.
    /// Loops (spins) until the lock is acquired.
    pub fn acquire(self: &mut Self) {
        push_off(); // disable interrupts to afn deadlock.
        if self.holding() {
            panic!("acquire");
        }

        // On RISC-V, sync_lock_test_and_set turns into an atomic swap:
        //   a5 = 1
        //   s1 = &lk->locked
        //   amoswap.w.aq a5, a5, (s1)
        while __sync_lock_test_and_set(&mut self.locked, 1) != 0 {}

        // Tell the C compiler and the processor to not move loads or stores
        // past this point, to ensure that the critical section's memory
        // references happen strictly after the lock is acquired.
        // On RISC-V, this emits a fence instruction.
        __sync_synchronize();

        // Record info about lock acquisition for holding() and debugging.
        self.cpu = Some(mycpu());
    }

    // Release the lock.
    pub fn release(self: &mut Self)
    {
        if !self.holding() {
            panic!("release");
        }

        self.cpu = None;

        // Tell the C compiler and the CPU to not move loads or stores
        // past this point, to ensure that all the stores in the critical
        // section are visible to other CPUs before the lock is released,
        // and that loads in the critical section occur strictly before
        // the lock is released.
        // On RISC-V, this emits a fence instruction.
        __sync_synchronize();

        // Release the lock, equivalent to lk->locked = 0.
        // This code doesn't use a C assignment, since the C standard
        // implies that an assignment might be implemented with
        // multiple store instructions.
        // On RISC-V, sync_lock_release turns into an atomic swap:
        //   s1 = &lk->locked
        //   amoswap.w zero, zero, (s1)
        __sync_lock_release(&self.locked);

        pop_off();
    }

    /// Check whether this cpu is holding the lock.
    /// Interrupts must be off.
    fn holding(self: &Self) -> bool {
        self.locked == 1 && self.cpu == Some(mycpu())
    }
}

/// push_off/pop_off are like intr_off()/intr_on() except that they are matched:
/// it takes two pop_off()s to undo two push_off()s.  Also, if interrupts
/// are initially off, then push_off, pop_off leaves them off.

pub fn push_off() {
    let old = intr_get();

    intr_off();
    let mut cpu = mycpu();
    unsafe {
        if (*cpu).noff == 0 {
            (*cpu).intena = old;
        }
        (*cpu).noff += 1;
    }
}

pub fn pop_off() {
    let cpu = mycpu();
    if intr_get() {
        panic!("pop_off - interruptible");
    }

    unsafe {
        if (*cpu).noff < 1 {
            panic!("pop_off");
        }
        (*cpu).noff -= 1;
        if (*cpu).noff == 0 && (*cpu).intena {
            intr_on();
        }
    }
}