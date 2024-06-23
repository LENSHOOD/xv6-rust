use crate::memlayout::{TRAMPOLINE, UART0_IRQ, VIRTIO0_IRQ};
use crate::plic::{plic_claim, plic_complete};
use crate::proc::Procstate::RUNNING;
use crate::proc::{cpuid, exit, killed, myproc, wakeup, yield_curr_proc};
use crate::riscv::{
    intr_get, intr_off, intr_on, r_satp, r_scause, r_sepc, r_sip, r_sstatus, r_stval, r_tp, w_sepc,
    w_sip, w_sstatus, w_stvec, PageTable, PGSIZE, SSTATUS_SPIE, SSTATUS_SPP,
};
use crate::spinlock::Spinlock;
use crate::syscall::syscall::syscall;
use crate::uart::UART_INSTANCE;
use crate::virtio::virtio_disk::virtio_disk_intr;
use crate::{printf, MAKE_SATP};

static mut TICKS_LOCK: Option<Spinlock> = None;
static mut TICKS: u32 = 0;

// in kernelvec.S, calls kerneltrap().
extern "C" {
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
    if (r_sstatus() & SSTATUS_SPP) != 0 {
        panic!("usertrap: not from user mode");
    }

    // send interrupts and exceptions to kerneltrap(),
    // since we're now in the kernel.
    w_stvec((unsafe { &kernelvec } as *const u8).expose_addr());

    let p = myproc();

    // save user program counter.
    let tf = unsafe { p.trapframe.unwrap().as_mut().unwrap() };
    tf.epc = r_sepc() as u64;

    let mut which_dev = 0;
    if r_scause() == 8 {
        // system call

        if killed(p) != 0 {
            exit(-1);
        }

        // sepc points to the ecall instruction,
        // but we want to return to the next instruction.
        tf.epc += 4;

        // an interrupt will change sepc, scause, and sstatus,
        // so enable only now that we're done with those registers.
        intr_on();

        syscall();
    } else {
        which_dev = devintr();
        if which_dev != 0 {
            // ok
        } else {
            printf!(
                "usertrap(): unexpected scause {:x} pid={}\n",
                r_scause(),
                p.pid
            );
            printf!("            sepc={:x} stval={:x}\n", r_sepc(), r_stval());
            p.setkilled();
        }
    }

    if killed(p) != 0 {
        exit(-1);
    }

    // give up the CPU if this is a timer interrupt.
    if which_dev == 2 {
        yield_curr_proc();
    }

    usertrapret();
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
    trapframe.kernel_satp = r_satp() as u64; // kernel page table
    trapframe.kernel_sp = (p.kstack + PGSIZE) as u64; // process's kernel stack
    trapframe.kernel_trap = usertrap as u64;
    trapframe.kernel_hartid = r_tp(); // hartid for cpuid()

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

// interrupts and exceptions from kernel code go here via kernelvec,
// on whatever the current kernel stack is.
#[no_mangle]
extern "C" fn kerneltrap() {
    let mut which_dev = 0;
    let sepc = r_sepc();
    let sstatus = r_sstatus();
    let scause = r_scause();

    if (sstatus & SSTATUS_SPP) == 0 {
        panic!("kerneltrap: not from supervisor mode");
    }
    if intr_get() {
        panic!("kerneltrap: interrupts enabled");
    }

    which_dev = devintr();
    if which_dev == 0 {
        printf!("scause {:x}\n", scause);
        printf!("sepc={:x} stval={:x}\n", r_sepc(), r_stval());
        panic!("kerneltrap");
    }

    let p = myproc();
    // give up the CPU if this is a timer interrupt.
    if which_dev == 2 && p.state == RUNNING {
        p.proc_yield();
    }

    // the yield() may have caused some traps to occur,
    // so restore trap registers for use by kernelvec.S's sepc instruction.
    w_sepc(sepc);
    w_sstatus(sstatus);
}

fn clockintr() {
    unsafe {
        let ticklock = &mut TICKS_LOCK.unwrap();
        ticklock.acquire();
        TICKS += 1;
        wakeup(&TICKS);
        ticklock.release();
    }
}

// check if it's an external interrupt or software interrupt,
// and handle it.
// returns 2 if timer interrupt,
// 1 if other device,
// 0 if not recognized.
fn devintr() -> u8 {
    let scause = r_scause();

    if (scause & 0x8000000000000000) != 0 && (scause & 0xff) == 9 {
        // this is a supervisor external interrupt, via PLIC.

        // irq indicates which device interrupted.
        let irq = plic_claim();

        if irq == UART0_IRQ as u32 {
            unsafe {
                UART_INSTANCE.intr();
            }
        } else if irq == VIRTIO0_IRQ as u32 {
            unsafe {
                virtio_disk_intr();
            }
        } else if irq == 0 {
            printf!("unexpected interrupt irq={}\n", irq);
        }

        // the PLIC allows each device to raise at most one
        // interrupt at a time; tell the PLIC the device is
        // now allowed to interrupt again.
        if irq != 0 {
            plic_complete(irq);
        }

        return 1;
    }

    if scause == 0x8000000000000001 {
        // software interrupt from a machine-mode timer interrupt,
        // forwarded by timervec in kernelvec.S.

        if cpuid() == 0 {
            clockintr();
        }

        // acknowledge the software interrupt by clearing
        // the SSIP bit in sip.
        w_sip(r_sip() & !2);

        return 2;
    }

    0
}
