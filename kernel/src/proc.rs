use core::f32::consts::PI;
use core::intrinsics::size_of;
use core::mem;
use core::sync::atomic::{AtomicU32, Ordering};
use crate::file::{File, INode};
use crate::fs::fs;
use crate::kalloc::KMEM;
use crate::KSTACK;
use crate::memlayout::{TRAMPOLINE, TRAPFRAME};
use crate::param::{NCPU, NOFILE, NPROC, ROOTDEV};
use crate::proc::Procstate::{RUNNABLE, UNUSED, USED};
use crate::riscv::{PageTable, PGSIZE, PTE_R, PTE_W, PTE_X, r_tp};
use crate::spinlock::{pop_off, push_off, Spinlock};
use crate::vm::{kvmmap, mappages, trampoline, uvmcreate, uvmfirst, uvmfree, uvmunmap};

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

static mut INIT_PROC: Option<&mut Proc> = None;

extern {
    static trampoline: u8; // trampoline.S
}

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
    lock: Spinlock,

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
    pagetable: Option<&'a mut PageTable>, // User page table
    trapframe: Option<&'a mut Trapframe>, // data page for trampoline.S
    context: Context, // swtch() here to run process
    ofile: Option<[&'a File<'a>; NOFILE]>, // Open files
    cwd: Option<&'a INode>,           // Current directory
    name: &'a str,               // Process name (debugging)
}

impl<'a> Proc<'a> {
    const fn default() -> Self {
        Proc {
            lock: Spinlock::init_lock("proc"),
            state: UNUSED,
            chan: None,
            killed: 0,
            xstate: 0,
            pid: 0,
            parent: None,
            kstack: KSTACK!(i),
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
            ofile: None,
            cwd: None,
            name: "",
        }
    }
}

static NEXT_PID: AtomicU32 = AtomicU32::new(1);
// helps ensure that wakeups of wait()ing
// parents are not lost. helps obey the
// memory model when using p->parent.
// must be acquired before any p->lock.
static mut WAIT_LOCK: Spinlock = Spinlock::init_lock("wait_lock");

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

fn allocpid() -> u32 {
    NEXT_PID.fetch_add(1, Ordering::Relaxed)
}

// Allocate a page for each process's kernel stack.
// Map it high in memory, followed by an invalid
// guard page.
pub fn proc_mapstacks(kpgtbl: &mut PageTable) {
    for idx in 0..NPROC {
        unsafe {
            let pa: *mut u8 = KMEM.kalloc();
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
    // empty due to PID_LOCK, WAIT_LOCK and PROCS has already been initialized
}

// a user program that calls exec("/init")
// assembled from ../user/initcode.S
// od -t xC ../user/initcode
const initcode: [u8; 52] = [
    0x17, 0x05, 0x00, 0x00, 0x13, 0x05, 0x45, 0x02,
    0x97, 0x05, 0x00, 0x00, 0x93, 0x85, 0x35, 0x02,
    0x93, 0x08, 0x70, 0x00, 0x73, 0x00, 0x00, 0x00,
    0x93, 0x08, 0x20, 0x00, 0x73, 0x00, 0x00, 0x00,
    0xef, 0xf0, 0x9f, 0xff, 0x2f, 0x69, 0x6e, 0x69,
    0x74, 0x00, 0x00, 0x24, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00
];

// Set up first user process.
fn userinit() {
    let p = allocproc();
    unsafe { INIT_PROC = p; }

    let p = p.unwrap();
    // allocate one user page and copy initcode's instructions
    // and data into it.
    uvmfirst(p.pagetable.unwrap(), &initcode as *const u8, mem::size_of_val(&initcode));
    p.sz = PGSIZE;

    // prepare for the very first "return" from kernel to user.
    p.trapframe.unwrap().epc = 0;      // user program counter
    p.trapframe.unwrap().sp = PGSIZE as u64;  // user stack pointer

    p.name = "initcode";
    // TODO
    // p.cwd = namei("/");

    p.state = RUNNABLE;

    p.lock.release();
}

// A fork child's very first scheduling by scheduler()
// will swtch to forkret.
fn forkret() {

    // Still holding p->lock from scheduler.
    &myproc().lock.release();

    let mut first = 1;
    if first {
    // File system initialization must be run in the context of a
    // regular process (e.g., because it calls sleep), and thus cannot
    // be run from main().
        first = 0;
        // TODO
        // fs::fsinit(ROOTDEV);
    }

    // TODO
    // usertrapret();
}

// Look in the process table for an UNUSED proc.
// If found, initialize state required to run in the kernel,
// and return with p->lock held.
// If there are no free procs, or a memory allocation fails, return 0.
fn allocproc() -> Option<&'static mut Proc> {
    let mut proc = None;
    for i in 0..NPROC {
        let p = unsafe { &mut PROCS[i] };
        p.lock.acquire();

        if p.state == UNUSED {
            proc = Some(p);
            break;
        }

        p.lock.release();
    }?;

    let p = proc?;
    p.pid = allocpid();
    p.state = USED;

    // Allocate a trapframe page.
    let trapframe_ptr = unsafe { KMEM.kalloc() };
    if trapframe_ptr.is_null() {
        freeproc(p);
        &p.lock.release();
        return None;
    }
    p.trapframe = Some(trapframe_ptr as &mut Trapframe);

    // An empty user page table.
    p.pagetable = proc_pagetable(p);
    if p.pagetable.is_none() {
        freeproc(p);
        p.lock.release();
        return None;
    }

    // Set up new context to start executing at forkret,
    // which returns to user space.
    p.context.ra = forkret as u64;
    p.context.sp = (p.kstack + PGSIZE) as u64;

    return Some(p);
}

// free a proc structure and the data hanging from it,
// including user pages.
// p->lock must be held.
fn freeproc(p: &mut Proc) {
    if let Some(tf) = p.trapframe {
        unsafe { KMEM.kfree(tf as *mut Trapframe) };
    }
    p.trapframe = None;

    if let Some(pgtabl) = p.pagetable {
        proc_freepagetable(p.pagetable.unwrap(), p.sz);
    }
    p.pagetable = None;

    p.sz = 0;
    p.pid = 0;
    p.parent = None;
    p.name = "";
    p.chan = None;
    p.killed = 0;
    p.xstate = 0;
    p.state = UNUSED;
}

// Create a user page table for a given process, with no user memory,
// but with trampoline and trapframe pages.
fn proc_pagetable<'a>(p: &Proc) -> Option<&'a mut PageTable> {
    // An empty page table.
    let pagetable = uvmcreate()?;


    // map the trampoline code (for system call return)
    // at the highest user virtual address.
    // only the supervisor uses it, on the way
    // to/from user space, so not PTE_U.
    let trapoline_addr = (unsafe { &trampoline } as *const u8).expose_addr();
    if mappages(pagetable, TRAMPOLINE, trapoline_addr, PGSIZE, PTE_R | PTE_X) != 0 {
        uvmfree(pagetable, 0);
        return None;
    }

    // map the trapframe page just below the trampoline page, for
    // trampoline.S.
    let trapframe_addr = (p.trapframe.unwrap() as *const Trapframe).expose_addr();
    if mappages(pagetable, TRAPFRAME, trapframe_addr, PGSIZE, PTE_R | PTE_W) < 0 {
        uvmunmap(pagetable, TRAMPOLINE, 1, 0);
        uvmfree(pagetable, 0);
        return None;
    }

    return Some(pagetable);
}

// Free a process's page table, and free the
// physical memory it refers to.
fn proc_freepagetable(pagetable: &mut PageTable, sz: usize) {
    uvmunmap(pagetable, TRAMPOLINE, 1, 0);
    uvmunmap(pagetable, TRAPFRAME, 1, 0);
    uvmfree(pagetable, sz);
}
