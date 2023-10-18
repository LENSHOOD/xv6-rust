use crate::kalloc::KMEM;
use crate::KSTACK;
use crate::param::{NCPU, NPROC};
use crate::proc::Procstate::UNUSED;
use crate::riscv::{PageTable, PGSIZE, PTE_R, PTE_W, r_tp};
use crate::spinlock::{pop_off, push_off, Spinlock};
use crate::vm::kvmmap;

// Saved registers for kernel context switches.
#[derive(Copy, Clone)]
struct Context {
    ra: u64,
    sp: u64,

    // callee-saved
    s0: u64,
    s1: u64,
    s2: u64,
    s3: u64,
    s4: u64,
    s5: u64,
    s6: u64,
    s7: u64,
    s8: u64,
    s9: u64,
    s10: u64,
    s11: u64,
}

// Per-CPU state.
#[derive(Copy, Clone)]
pub struct Cpu<'a> {
    proc: Option<&'a Proc<'a>>,
    // The process running on this cpu, or null.
    context: Option<Context>,
    // swtch() here to enter scheduler().
    pub noff: u8,
    // Depth of push_off() nesting.
    pub intena: bool,          // Were interrupts enabled before push_off()?
}

impl<'a> Cpu<'a> {
    const fn default() -> Self {
        Cpu {
            proc: None,
            context: None,
            noff: 0,
            intena: false,
        }
    }
}

static mut CPUS: [Cpu; NCPU] = [Cpu::default(); NCPU];
static mut PROCS: [Proc; NPROC] = [Proc::default(); NPROC];

// per-process data for the trap handling code in trampoline.S.
// sits in a page by itself just under the trampoline page in the
// user page table. not specially mapped in the kernel page table.
// uservec in trampoline.S saves user registers in the trapframe,
// then initializes registers from the trapframe's
// kernel_sp, kernel_hartid, kernel_satp, and jumps to kernel_trap.
// usertrapret() and userret in trampoline.S set up
// the trapframe's kernel_*, restore user registers from the
// trapframe, switch to the user page table, and enter user space.
// the trapframe includes callee-saved user registers like s0-s11 because the
// return-to-user path via usertrapret() doesn't return through
// the entire kernel call stack.
struct Trapframe {
    /*   0 */ kernel_satp: u64,
    // kernel page table
    /*   8 */ kernel_sp: u64,
    // top of process's kernel stack
    /*  16 */ kernel_trap: u64,
    // usertrap()
    /*  24 */ epc: u64,
    // saved user program counter
    /*  32 */ kernel_hartid: u64,
    // saved kernel tp
    /*  40 */ ra: u64,
    /*  48 */ sp: u64,
    /*  56 */ gp: u64,
    /*  64 */ tp: u64,
    /*  72 */ t0: u64,
    /*  80 */ t1: u64,
    /*  88 */ t2: u64,
    /*  96 */ s0: u64,
    /* 104 */ s1: u64,
    /* 112 */ a0: u64,
    /* 120 */ a1: u64,
    /* 128 */ a2: u64,
    /* 136 */ a3: u64,
    /* 144 */ a4: u64,
    /* 152 */ a5: u64,
    /* 160 */ a6: u64,
    /* 168 */ a7: u64,
    /* 176 */ s2: u64,
    /* 184 */ s3: u64,
    /* 192 */ s4: u64,
    /* 200 */ s5: u64,
    /* 208 */ s6: u64,
    /* 216 */ s7: u64,
    /* 224 */ s8: u64,
    /* 232 */ s9: u64,
    /* 240 */ s10: u64,
    /* 248 */ s11: u64,
    /* 256 */ t3: u64,
    /* 264 */ t4: u64,
    /* 272 */ t5: u64,
    /* 280 */ t6: u64,
}

#[derive(Copy, Clone)]
enum Procstate { UNUSED, USED, SLEEPING, RUNNABLE, RUNNING, ZOMBIE }

// Per-process state
#[derive(Copy, Clone)]
pub struct Proc<'a> {
    lock: Option<Spinlock>,

    // p->lock must be held when using these:
    state: Procstate, // Process state
    chan: Option<*const u8>, // If non-zero, sleeping on chan
    killed: u8, // If non-zero, have been killed
    xstate: u8, // Exit status to be returned to parent's wait
    pub pid: u32,                     // Process ID

    // wait_lock must be held when using this:
    parent: Option<&'a Proc<'a>>,         // Parent process

    // these are private to the process, so p->lock need not be held.
    kstack: usize, // Virtual address of kernel stack
    sz: usize, // Size of process memory (bytes)
    pagetable: Option<&'a PageTable>, // User page table
    trapframe: Option<&'a Trapframe>, // data page for trampoline.S
    context: Context, // swtch() here to run process
    // TODO: open file
    // ofile: [&'own File; NOFILE], // Open files
    // TODO: inode
    // struct inode *cwd;           // Current directory
    name: &'a str,               // Process name (debugging)
}

impl<'a> Proc<'a> {
    const fn default() -> Self {
        Proc {
            lock: None,
            state: UNUSED,
            chan: None,
            killed: 0,
            xstate: 0,
            pid: 0,
            parent: None,
            kstack: 0,
            sz: 0,
            pagetable: None,
            trapframe: None,
            context: Context {
                ra: 0,
                sp: 0,
                s0: 0,
                s1: 0,
                s2: 0,
                s3: 0,
                s4: 0,
                s5: 0,
                s6: 0,
                s7: 0,
                s8: 0,
                s9: 0,
                s10: 0,
                s11: 0,
            },
            name: "",
        }
    }
}

static nextpid: u32 = 1;
static mut PID_LOCK: Option<Spinlock> = None;
// helps ensure that wakeups of wait()ing
// parents are not lost. helps obey the
// memory model when using p->parent.
// must be acquired before any p->lock.
static mut WAIT_LOCK: Option<Spinlock> = None;

// Must be called with interrupts disabled,
// to prevent race with process being moved
// to a different CPU.
pub fn cpuid() -> usize {
    r_tp() as usize
}

// Return this CPU's cpu struct.
// Interrupts must be disabled.
pub fn mycpu() -> &'static mut Cpu<'static> {
    unsafe {
        &mut CPUS[cpuid()]
    }
}

// Return the current struct proc *, or zero if none.
pub fn myproc<'a>() -> &'a Proc<'a> {
    push_off();
    let c = mycpu();
    let p = &c.proc;
    pop_off();
    p.unwrap()
}

// Allocate a page for each process's kernel stack.
// Map it high in memory, followed by an invalid
// guard page.
pub fn proc_mapstacks(kpgtbl: &mut PageTable) {
    for idx in 0..NPROC {
        unsafe {
            let pa = KMEM.as_mut().unwrap().kalloc();
            if pa.is_null() {
                panic!("kalloc");
            }
            let va = KSTACK!(idx);
            kvmmap(kpgtbl, va, pa as usize, PGSIZE, PTE_R | PTE_W)
        }
    }
}

// initialize the proc table.
pub fn procinit() {
    unsafe {
        PID_LOCK = Some(Spinlock::init_lock("nextpid"));
        WAIT_LOCK = Some(Spinlock::init_lock("wait_lock"));

        for i in 0..NPROC {
            let p = &mut PROCS[i];
            p.lock = Some(Spinlock::init_lock("proc"));
            p.state = UNUSED;
            p.kstack = KSTACK!(i)
        }
    }
}

