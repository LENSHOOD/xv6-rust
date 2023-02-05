#![no_std]
#![no_main]
#![feature(panic_info_message)]

mod asm;
mod riscv;
mod memlayout;
mod param;
mod uart;

use core::ops::Add;
use crate::memlayout::CLINT_MTIME;
use crate::riscv::*;
use crate::param::*;

// ///////////////////////////////////
// / RUST MACROS
// ///////////////////////////////////
#[macro_export]
macro_rules! print
{
	($($args:tt)+) => ({
        use core::fmt::Write;
        let _ = write!(crate::uart::UartDriver::new(0x1000_0000), $($args)+);
	});
}
#[macro_export]
macro_rules! println
{
	() => ({
		print!("\r\n")
	});
	($fmt:expr) => ({
		print!(concat!($fmt, "\r\n"))
	});
	($fmt:expr, $($args:tt)+) => ({
		print!(concat!($fmt, "\r\n"), $($args)+)
	});
}

// ///////////////////////////////////
// / LANGUAGE STRUCTURES / FUNCTIONS
// ///////////////////////////////////
#[no_mangle]
extern "C" fn eh_personality() {}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    print!("Aborting: ");
    if let Some(p) = info.location() {
        println!(
            "line {}, file {}: {}",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    }
    else {
        println!("no information available.");
    }
    abort();
}

#[no_mangle]
extern "C"
fn abort() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi")
        }
    }
}

static timer_scratch: [[u64; NCPU]; 5] = [[0; NCPU]; 5];

#[repr(C, align(16))]
struct Stack0Aligned([u8; 4096 * NCPU]);
#[no_mangle]
static stack0: Stack0Aligned = Stack0Aligned([0; 4096 * NCPU]);

#[no_mangle]
extern "C"
fn start() {
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
    timerinit();

    // keep each CPU's hartid in its tp register, for cpuid().
    let id = r_mhartid();
    w_tp(id);

    // switch to supervisor mode and jump to main().
    unsafe {
        core::arch::asm!("mret")
    }
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
        (CLINT_MTIMECMP!(id) as *mut u64).write_volatile((CLINT_MTIME as *const u64).read_volatile() + interval)
    }

    // prepare information in scratch[] for timervec.
    // scratch[0..2] : space for timervec to save registers.
    // scratch[3] : address of CLINT MTIMECMP register.
    // scratch[4] : desired interval (in cycles) between timer interrupts.
    let mut scratch = timer_scratch[id as usize];
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

#[no_mangle]
extern "C"
fn kmain() {
    let mut my_uart = uart::UartDriver::new(0x1000_0000);
    my_uart.init();

    println!("This is my operating system!");
    println!("I'm so awesome. If you start typing something, I'll show you what you typed!");
}

#[no_mangle]
extern "C" fn kinit_hart(_hartid: usize) {
    // We aren't going to do anything here until we get SMP going.
    // All non-0 harts initialize here.
}