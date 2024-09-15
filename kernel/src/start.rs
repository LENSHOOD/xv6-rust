use core::arch::asm;

use crate::{CLINT_MTIMECMP, kmain};
use crate::memlayout::CLINT_MTIME;
use crate::param::*;
use crate::riscv::*;

static TIMER_SCRATCH: [[u64; NCPU]; 5] = [[0; NCPU]; 5];

#[repr(C, align(16))]
struct Stack0Aligned([u8; 4096 * NCPU]);
#[no_mangle]
static stack0: Stack0Aligned = Stack0Aligned([0; 4096 * NCPU]);

#[no_mangle]
extern "C" fn start() {
    // set M Previous Privilege mode to Supervisor, for mret.
    let mut x = r_mstatus();
    x &= !MSTATUS_MPP_MASK;
    x |= MSTATUS_MPP_S;
    w_mstatus(x);

    // set M Exception Program Counter to main, for mret.
    // requires gcc -mcmodel=medany
    w_mepc(kmain as usize);

    // disable paging for now.
    w_satp(0);

    // delegate all interrupts and exceptions to supervisor mode.
    w_medeleg(0xffff);
    w_mideleg(0xffff);
    w_sie(r_sie() | SIE_SEIE | SIE_STIE | SIE_SSIE);

    // configure Physical Memory Protection to give supervisor mode
    // access to all of physical memory.
    w_pmpaddr0(0x3ffffffffffff);
    w_pmpcfg0(0xf);

    // ask for clock interrupts.
    // timerinit();

    // keep each CPU's hartid in its tp register, for cpuid().
    let id = r_mhartid();
    w_tp(id);

    // switch to supervisor mode and jump to main().
    unsafe { asm!("mret") }
}

extern "C" {
    fn timervec();
}

fn timerinit() {
    // each CPU has a separate source of timer interrupts.
    let id = r_mhartid();

    // ask the CLINT for a timer interrupt.
    let interval = 1000000; // cycles; about 1/10th second in qemu.
    unsafe {
        (CLINT_MTIMECMP!(id) as *mut u64)
            .write_volatile((CLINT_MTIME as *const u64).read_volatile() + interval)
    }

    // prepare information in scratch[] for timervec.
    // scratch[0..2] : space for timervec to save registers.
    // scratch[3] : address of CLINT MTIMECMP register.
    // scratch[4] : desired interval (in cycles) between timer interrupts.
    let mut scratch = TIMER_SCRATCH[id as usize];
    scratch[3] = CLINT_MTIMECMP!(id);
    scratch[4] = interval;
    let raw = &scratch as *const u64;
    w_mscratch(raw as usize);

    // set the machine-mode trap handler.
    w_mtvec(timervec as usize);

    // enable machine-mode interrupts.
    w_mstatus(r_mstatus() | MSTATUS_MIE);

    // enable machine-mode timer interrupts.
    w_mie(r_mie() | MIE_MTIE);
}
