use crate::riscv::w_stvec;
use crate::spinlock::Spinlock;


static mut TICKS_LOCK: Option<Spinlock> = None;

// in kernelvec.S, calls kerneltrap().
extern {
    static kernelvec: u8;
}

pub fn trapinit() {
    unsafe {
        TICKS_LOCK = Some(Spinlock::init_lock("time"));
    }
}

// set up to take exceptions and traps while in the kernel.
pub fn trapinithart() {
    w_stvec((unsafe { &kernelvec } as *const u8).expose_addr());
}
