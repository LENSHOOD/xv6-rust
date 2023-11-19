use crate::MAKE_SATP;
use crate::memlayout::TRAMPOLINE;
use crate::proc::myproc;
use crate::riscv::{intr_off, PageTable, PGSIZE, r_satp, r_sstatus, r_tp, SSTATUS_SPIE, SSTATUS_SPP, w_sepc, w_sstatus, w_stvec};
use crate::spinlock::Spinlock;


static mut TICKS_LOCK: Option<Spinlock> = None;

// in kernelvec.S, calls kerneltrap().
extern {
    static kernelvec: u8;
    static trampoline: u8;
    static uservec: u8;
    static userret: u8;
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

//
// handle an interrupt, exception, or system call from user space.
// called from trampoline.S
//
fn usertrap() {
    // TODO: migrate
    panic!("unimplemented")
}

//
// return to user space
//
pub fn usertrapret() {
    let p = myproc();

    // we're about to switch the destination of traps from
    // kerneltrap() to usertrap(), so turn off interrupts until
    // we're back in user space, where usertrap() is correct.
    intr_off();

    // send syscalls, interrupts, and exceptions to uservec in trampoline.S
    let uservec_addr = (unsafe { &uservec } as *const u8).expose_addr();
    let trampoline_addr = (unsafe { &trampoline } as *const u8).expose_addr();
    let trampoline_uservec = TRAMPOLINE + uservec_addr - trampoline_addr;
    w_stvec(trampoline_uservec);

    // set up trapframe values that uservec will need when
    // the process next traps into the kernel.

    let trapframe = unsafe { p.trapframe.unwrap().as_mut().unwrap() };
    trapframe.kernel_satp = r_satp() as u64;         // kernel page table
    trapframe.kernel_sp = (p.kstack + PGSIZE) as u64; // process's kernel stack
    trapframe.kernel_trap = usertrap as u64;
    trapframe.kernel_hartid = r_tp();         // hartid for cpuid()

    // set up the registers that trampoline.S's sret will use
    // to get to user space.

    // set S Previous Privilege mode to User.
    let mut x = r_sstatus();
    x &= !SSTATUS_SPP; // clear SPP to 0 for user mode
    x |= SSTATUS_SPIE; // enable interrupts in user mode
    w_sstatus(x);

    // set S Exception Program Counter to the saved user pc.
    w_sepc(trapframe.epc as usize);

    // tell trampoline.S the user page table to switch to.
    let satp = MAKE_SATP!((p.pagetable.unwrap() as *const PageTable).expose_addr());

    // jump to userret in trampoline.S at the top of memory, which
    // switches to the user page table, restores user registers,
    // and switches to user mode with sret.
    let userret_addr = (unsafe { &userret } as *const u8).expose_addr();
    let trampoline_userret = TRAMPOLINE + userret_addr - trampoline_addr;

    unsafe {
        let func = *(trampoline_userret as *const fn(stap: usize));
        func(satp);
    };
}
