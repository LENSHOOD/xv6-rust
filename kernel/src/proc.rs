use crate::file::file::fileclose;
use crate::file::{File, INode};
use crate::fs::fs;
use crate::fs::fs::namei;
use crate::kalloc::KMEM;
use crate::log::{begin_op, end_op};
use crate::memlayout::{TRAMPOLINE, TRAPFRAME};
use crate::param::{NCPU, NOFILE, NPROC, ROOTDEV};
use crate::proc::Procstate::{RUNNABLE, RUNNING, SLEEPING, UNUSED, USED, ZOMBIE};
use crate::riscv::{intr_get, intr_on, r_tp, PageTable, PGSIZE, PTE_R, PTE_W, PTE_X};
use crate::spinlock::{pop_off, push_off, Spinlock};
use crate::string::memmove;
use crate::trap::usertrapret;
use crate::vm::{copyin, copyout, kvmmap, mappages, uvmcreate, uvmfirst, uvmfree, uvmunmap};
use crate::{printf, KSTACK};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use core::{mem, ptr};

// Saved registers for kernel context switches.
#[repr(C)]
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

impl Context {
    pub(crate) const fn default() -> Self {
        Self {
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
        }
    }
}

// Per-CPU state.
#[derive(Copy, Clone)]
pub struct Cpu<'a> {
    proc: Option<*mut Proc<'a>>,
    // The process running on this cpu, or null.
    context: Context,
    // swtch() here to enter scheduler().
    pub noff: u8,
    // Depth of push_off() nesting.
    pub intena: bool, // Were interrupts enabled before push_off()?
}

impl<'a> Cpu<'a> {
    const fn default() -> Self {
        Cpu {
            proc: None,
            context: Context::default(),
            noff: 0,
            intena: false,
        }
    }
}

static mut CPUS: [Cpu; NCPU] = [Cpu::default(); NCPU];
static mut PROCS: [Proc; NPROC] = [Proc::default(); NPROC];

static mut INIT_PROC: Option<&mut Proc> = None;

extern "C" {
    static trampoline: u8; // trampoline.S
    fn swtch(curr_ctx: &Context, backup_ctx: &Context);
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
#[derive(Copy, Clone)]
pub struct Trapframe {
    /*   0 */ pub(crate) kernel_satp: u64,
    // kernel page table
    /*   8 */ pub(crate) kernel_sp: u64,
    // top of process's kernel stack
    /*  16 */ pub(crate) kernel_trap: u64,
    // usertrap()
    /*  24 */ pub(crate) epc: u64,
    // saved user program counter
    /*  32 */ pub(crate) kernel_hartid: u64,
    // saved kernel tp
    /*  40 */ ra: u64,
    /*  48 */ pub(crate) sp: u64,
    /*  56 */ gp: u64,
    /*  64 */ tp: u64,
    /*  72 */ t0: u64,
    /*  80 */ t1: u64,
    /*  88 */ t2: u64,
    /*  96 */ s0: u64,
    /* 104 */ s1: u64,
    /* 112 */ pub(crate) a0: u64,
    /* 120 */ pub(crate) a1: u64,
    /* 128 */ pub(crate) a2: u64,
    /* 136 */ pub(crate) a3: u64,
    /* 144 */ pub(crate) a4: u64,
    /* 152 */ pub(crate) a5: u64,
    /* 160 */ a6: u64,
    /* 168 */ pub(crate) a7: u64,
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

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum Procstate {
    UNUSED,
    USED,
    SLEEPING,
    RUNNABLE,
    RUNNING,
    ZOMBIE,
}

// Per-process state
#[derive(Copy, Clone)]
pub struct Proc<'a> {
    pub(crate) lock: Spinlock,

    // p->lock must be held when using these:
    pub(crate) state: Procstate, // Process state
    chan: Option<*const u8>,     // If non-zero, sleeping on chan
    killed: u8,                  // If non-zero, have been killed
    xstate: u8,                  // Exit status to be returned to parent's wait
    pub pid: u32,                // Process ID

    // wait_lock must be held when using this:
    pub(crate) parent: Option<&'a Proc<'a>>, // Parent process

    // these are private to the process, so p->lock need not be held.
    pub(crate) kstack: usize, // Virtual address of kernel stack
    pub(crate) sz: usize,     // Size of process memory (bytes)
    pub(crate) pagetable: Option<*mut PageTable>, // User page table
    pub(crate) trapframe: Option<*mut Trapframe>, // data page for trampoline.S
    context: Context,         // swtch() here to run process
    pub(crate) ofile: [Option<*mut File>; NOFILE], // Open files
    pub(crate) cwd: Option<*mut INode>, // Current directory
    pub(crate) name: [u8; 16], // Process name (debugging)
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
            kstack: 0,
            sz: 0,
            pagetable: None,
            trapframe: None,
            context: Context::default(),
            ofile: [None; NOFILE],
            cwd: None,
            name: [0; 16],
        }
    }

    pub(crate) fn setkilled(self: &mut Self) {
        self.lock.acquire();
        self.killed = 1;
        self.lock.release();
    }

    pub(crate) fn proc_yield(self: &mut Self) {
        self.lock.acquire();
        self.state = RUNNABLE;
        sched();
        self.lock.release();
    }
}

static NEXT_PID: AtomicU32 = AtomicU32::new(1);
// helps ensure that wakeups of wait()ing
// parents are not lost. helps obey the
// memory model when using p->parent.
// must be acquired before any p->lock.
pub(crate) static mut WAIT_LOCK: Spinlock = Spinlock::init_lock("wait_lock");

// Must be called with interrupts disabled,
// to prevent race with process being moved
// to a different CPU.
pub fn cpuid() -> usize {
    r_tp() as usize
}

// Return this CPU's cpu struct.
// Interrupts must be disabled.
pub fn mycpu() -> &'static mut Cpu<'static> {
    unsafe { &mut CPUS[cpuid()] }
}

static mut DUMMY_PROC: Proc = Proc::default();
// Return the current struct proc *, or zero if none.
// Here we return a dummy Proc if no proc on cpu.
pub fn myproc() -> &'static mut Proc<'static> {
    push_off();
    let c = mycpu();
    let p = c.proc.unwrap_or(unsafe { &mut DUMMY_PROC as *mut Proc });
    pop_off();
    unsafe { p.as_mut().unwrap() }
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
            // 1st page of kernel stack
            let pa_0: *mut u8 = KMEM.kalloc();
            // 2nd page of kernel stack
            let pa_1: *mut u8 = KMEM.kalloc();
            if pa_0.is_null() || pa_1.is_null() {
                panic!("kalloc");
            }
            let va = KSTACK!(idx);
            kvmmap(kpgtbl, va, pa_0 as usize, PGSIZE, PTE_R | PTE_W);
            kvmmap(kpgtbl, va + PGSIZE, pa_1 as usize, PGSIZE, PTE_R | PTE_W);
        }
    }
}

// initialize the proc table.
pub fn procinit() {
    for i in 0..NPROC {
        unsafe { PROCS[i].kstack = KSTACK!(i) }
    }
}

// a user program that calls exec("/init")
// assembled from ../user/initcode.S
// od -t xC ../user/initcode
const INIT_CODE: [u8; 52] = [
    0x17, 0x05, 0x00, 0x00, 0x13, 0x05, 0x45, 0x02, 0x97, 0x05, 0x00, 0x00, 0x93, 0x85, 0x35, 0x02,
    0x93, 0x08, 0x70, 0x00, 0x73, 0x00, 0x00, 0x00, 0x93, 0x08, 0x20, 0x00, 0x73, 0x00, 0x00, 0x00,
    0xef, 0xf0, 0x9f, 0xff, 0x2f, 0x69, 0x6e, 0x69, 0x74, 0x00, 0x00, 0x24, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00,
];

// Set up first user process.
pub fn userinit() {
    let p = allocproc().unwrap();
    // allocate one user page and copy initcode's instructions
    // and data into it.
    uvmfirst(
        unsafe { p.pagetable.unwrap().as_mut().unwrap() },
        &INIT_CODE as *const u8,
        mem::size_of_val(&INIT_CODE),
    );
    p.sz = PGSIZE;

    // prepare for the very first "return" from kernel to user.
    unsafe {
        p.trapframe.unwrap().as_mut().unwrap().epc = 0; // user program counter
        p.trapframe.unwrap().as_mut().unwrap().sp = PGSIZE as u64; // user stack pointer
    }

    let mut name = [0; 16];
    name.copy_from_slice("initcode\0\0\0\0\0\0\0\0".as_bytes());
    p.name = name;
    p.cwd = namei(&[b'/']).map(|inner| inner as *mut INode);

    p.state = RUNNABLE;

    p.lock.release();

    unsafe {
        INIT_PROC = Some(p);
    }
}

// Give up the CPU for one scheduling round.
pub(crate) fn yield_curr_proc() {
    myproc().proc_yield();
}

// A fork child's very first scheduling by scheduler()
// will swtch to forkret.
const FIRST: AtomicBool = AtomicBool::new(true);
fn forkret() {
    // Still holding p->lock from scheduler.
    let my_proc = myproc();
    my_proc.lock.release();

    if FIRST.load(Ordering::Relaxed) {
        // File system initialization must be run in the context of a
        // regular process (e.g., because it calls sleep), and thus cannot
        // be run from main().
        FIRST.store(false, Ordering::Relaxed);
        fs::fsinit(ROOTDEV);
    }

    usertrapret();
}

// Look in the process table for an UNUSED proc.
// If found, initialize state required to run in the kernel,
// and return with p->lock held.
// If there are no free procs, or a memory allocation fails, return 0.
pub(crate) fn allocproc() -> Option<&'static mut Proc<'static>> {
    for p in unsafe { &mut PROCS } {
        p.lock.acquire();

        if p.state == UNUSED {
            return inner_alloc(p);
        }

        p.lock.release();
    }

    None
}

fn inner_alloc<'a>(p: &'a mut Proc<'a>) -> Option<&'a mut Proc<'a>> {
    p.pid = allocpid();
    p.state = USED;

    // Allocate a trapframe page.
    let trapframe_ptr: *mut Trapframe = unsafe { KMEM.kalloc() };
    if trapframe_ptr.is_null() {
        freeproc(p);
        p.lock.release();
        return None;
    }
    p.trapframe = Some(trapframe_ptr);

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
    p.context.sp = (p.kstack + 2 * PGSIZE) as u64;
    Some(p)
}

// free a proc structure and the data hanging from it,
// including user pages.
// p->lock must be held.
pub(crate) fn freeproc(p: &mut Proc) {
    if let Some(tf) = p.trapframe {
        unsafe { KMEM.kfree(tf) };
    }
    p.trapframe = None;

    if let Some(pgtabl) = p.pagetable {
        proc_freepagetable(unsafe { pgtabl.as_mut().unwrap() }, p.sz);
    }
    p.pagetable = None;

    p.sz = 0;
    p.pid = 0;
    p.parent = None;
    p.name = [0; 16];
    p.chan = None;
    p.killed = 0;
    p.xstate = 0;
    p.state = UNUSED;
}

// Create a user page table for a given process, with no user memory,
// but with trampoline and trapframe pages.
pub fn proc_pagetable<'a>(p: &Proc) -> Option<*mut PageTable> {
    // An empty page table.
    let pagetable = uvmcreate()?;

    // map the trampoline code (for system call return)
    // at the highest user virtual address.
    // only the supervisor uses it, on the way
    // to/from user space, so not PTE_U.
    let trampoline_addr = (unsafe { &trampoline } as *const u8).expose_addr();
    if mappages(
        pagetable,
        TRAMPOLINE,
        trampoline_addr,
        PGSIZE,
        PTE_R | PTE_X,
    ) != 0
    {
        uvmfree(pagetable, 0);
        return None;
    }

    // map the trapframe page just below the trampoline page, for
    // trampoline.S.
    let trapframe_addr = (*p.trapframe.as_ref().unwrap() as *const Trapframe).expose_addr();
    if mappages(pagetable, TRAPFRAME, trapframe_addr, PGSIZE, PTE_R | PTE_W) < 0 {
        uvmunmap(pagetable, TRAMPOLINE, 1, false);
        uvmfree(pagetable, 0);
        return None;
    }

    return Some(pagetable);
}

// Free a process's page table, and free the
// physical memory it refers to.
pub fn proc_freepagetable(pagetable: &mut PageTable, sz: usize) {
    uvmunmap(pagetable, TRAMPOLINE, 1, false);
    uvmunmap(pagetable, TRAPFRAME, 1, false);
    uvmfree(pagetable, sz);
}

pub(crate) fn killed(p: &mut Proc) -> u8 {
    p.lock.acquire();
    let k = p.killed;
    p.lock.release();
    return k;
}

// Copy to either a user address, or kernel address,
// depending on usr_dst.
// Returns 0 on success, -1 on error.
pub(crate) fn either_copyout(is_user_dst: bool, dst: *mut u8, src: *const u8, len: usize) -> i8 {
    let p = myproc();
    if is_user_dst {
        copyout(
            unsafe { p.pagetable.unwrap().as_mut().unwrap() },
            dst.expose_addr(),
            src,
            len,
        )
    } else {
        memmove(dst, src, len);
        return 0;
    }
}

// Copy from either a user address, or kernel address,
// depending on usr_src.
// Returns 0 on success, -1 on error.
pub(crate) fn either_copyin(dst: *mut u8, is_user_src: bool, src: *const u8, len: usize) -> i8 {
    let p = myproc();
    if is_user_src {
        copyin(
            unsafe { p.pagetable.unwrap().as_mut().unwrap() },
            dst,
            src.expose_addr(),
            len,
        )
    } else {
        memmove(dst, src, len);
        return 0;
    }
}

// Wake up all processes sleeping on chan.
// Must be called without any p->lock.
pub(crate) fn wakeup<T>(chan: &T) {
    for p in unsafe { &mut PROCS } {
        if p as *const Proc != myproc() as *const Proc {
            p.lock.acquire();
            if p.state == SLEEPING && p.chan == Some(chan as *const T as *const u8) {
                p.state = RUNNABLE;
            }
            p.lock.release()
        }
    }
}

// Atomically release lock and sleep on chan.
// Reacquires lock when awakened.
pub fn sleep<T>(chan: *const T, lk: &mut Spinlock) {
    let p = myproc();

    // Must acquire p->lock in order to
    // change p->state and then call sched.
    // Once we hold p->lock, we can be
    // guaranteed that we won't miss any wakeup
    // (wakeup locks p->lock),
    // so it's okay to release lk.

    p.lock.acquire(); //DOC: sleeplock1
    lk.release();

    // Go to sleep.
    p.chan = Some(chan as *const u8);
    p.state = SLEEPING;

    sched();

    // Tidy up.
    p.chan = None;

    // Reacquire original lock.
    p.lock.release();
    lk.acquire();
}

// Per-CPU process scheduler.
// Each CPU calls scheduler() after setting itself up.
// Scheduler never returns.  It loops, doing:
//  - choose a process to run.
//  - swtch to start running that process.
//  - eventually that process transfers control
//    via swtch back to the scheduler.
pub fn scheduler() {
    let c = mycpu();

    c.proc = None;
    loop {
        // Avoid deadlock by ensuring that devices can interrupt.
        intr_on();

        for p in unsafe { &mut PROCS } {
            p.lock.acquire();
            if p.state == RUNNABLE {
                // Switch to chosen process.  It is the process's job
                // to release its lock and then reacquire it
                // before jumping back to us.
                p.state = RUNNING;
                c.proc = Some(p);
                unsafe { swtch(&c.context, &p.context) }

                // Process is done running for now.
                // It should have changed its p->state before coming back.
                c.proc = None;
            }
            p.lock.release();
        }
    }
}

// Switch to scheduler.  Must hold only p->lock
// and have changed proc->state. Saves and restores
// intena because intena is a property of this
// kernel thread, not this CPU. It should
// be proc->intena and proc->noff, but that would
// break in the few places where a lock is held but
// there's no process.
fn sched() {
    let p = myproc();

    if !p.lock.holding() {
        panic!("sched p->lock");
    }

    if mycpu().noff != 1 {
        panic!("sched locks");
    }

    if p.state == RUNNING {
        panic!("sched running");
    }

    if intr_get() {
        panic!("sched interruptible");
    }

    let intena = mycpu().intena;
    unsafe {
        swtch(&p.context, &mycpu().context);
    }
    mycpu().intena = intena;
}

// Exit the current process.  Does not return.
// An exited process remains in the zombie state
// until its parent calls wait().
pub(crate) fn exit(status: i32) {
    let p = myproc();

    if ptr::eq(p, unsafe { *INIT_PROC.as_ref().unwrap() }) {
        panic!("init exiting");
    }

    // Close all open files.
    for fd in 0..NOFILE {
        if p.ofile[fd].is_some() {
            let f = p.ofile[fd].unwrap();
            fileclose(unsafe { f.as_mut().unwrap() });
            p.ofile[fd] = None;
        }
    }

    begin_op();
    unsafe {
        p.cwd.unwrap().as_mut().unwrap().iput();
    }
    end_op();
    p.cwd = None;

    unsafe {
        WAIT_LOCK.acquire();
    }

    // Give any children to init.
    reparent(p);

    // Parent might be sleeping in wait().
    wakeup(p.parent.unwrap());

    p.lock.acquire();
    p.xstate = status as u8;
    p.state = ZOMBIE;

    unsafe {
        WAIT_LOCK.release();
    }

    // Jump into the scheduler, never to return.
    sched();
    panic!("zombie exit");
}

// Wait for a child process to exit and return its pid.
// Return -1 if this process has no children.
pub(crate) fn wait(addr: usize) -> i32 {
    let p = myproc();

    unsafe {
        WAIT_LOCK.acquire();
    }

    let mut havekids = false;
    loop {
        for i in 0..NPROC {
            let pp = unsafe { &mut PROCS[i] };
            if pp.parent.is_some() && pp.parent.unwrap() as *const Proc == p as *const Proc {
                // make sure the child isn't still in exit() or swtch().
                pp.lock.acquire();

                havekids = true;
                if pp.state == ZOMBIE {
                    // Found one.
                    let pid = pp.pid;
                    if addr != 0
                        && copyout(
                            unsafe { p.pagetable.unwrap().as_mut().unwrap() },
                            addr,
                            &pp.xstate as *const u8,
                            mem::size_of_val(&pp.xstate),
                        ) < 0
                    {
                        pp.lock.release();
                        unsafe {
                            WAIT_LOCK.release();
                        }
                        return -1;
                    }

                    freeproc(pp);
                    pp.lock.release();
                    unsafe {
                        WAIT_LOCK.release();
                    }
                    return pid as i32;
                }

                pp.lock.release();
            }
        }

        // No point waiting if we don't have any children.
        if !havekids || killed(p) != 0 {
            unsafe {
                WAIT_LOCK.release();
            }
            return -1;
        }

        // Wait for a child to exit.
        sleep(p, unsafe { &mut WAIT_LOCK }); //DOC: wait-sleep
    }
}

// Pass p's abandoned children to init.
// Caller must hold wait_lock.
fn reparent(p: &mut Proc) {
    unsafe {
        for i in 0..NPROC {
            let pp = &mut PROCS[i];
            if pp.parent.is_some() {
                if ptr::eq(pp.parent.unwrap(), p) {
                    pp.parent = Some(&INIT_PROC.as_ref().unwrap());
                    wakeup(&INIT_PROC);
                };
            }
        }
    }
}

// Print a process listing to console.  For debugging.
// Runs when user types ^P on console.
// No lock to avoid wedging a stuck machine further.
pub(crate) fn procdump() {
    printf!("\n");
    for i in 0..NPROC {
        let p = unsafe { &PROCS[i] };
        if p.state == UNUSED {
            continue;
        }

        let state = match p.state {
            UNUSED => continue,
            USED => "used",
            SLEEPING => "sleep ",
            RUNNABLE => "runble",
            RUNNING => "run   ",
            ZOMBIE => "zombie",
        };

        printf!(
            "{} {} {}",
            p.pid,
            state,
            core::str::from_utf8(&p.name).unwrap()
        );
        printf!("\n");
    }
}
