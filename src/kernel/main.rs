#![no_std]
#![no_main]
#![feature(panic_info_message)]

extern crate alloc;

mod asm;
mod riscv;
mod memlayout;
mod param;
mod uart;
mod start;
mod spinlock;
mod proc;
mod console;
mod printf;

use core::alloc::{GlobalAlloc, Layout};
use core::fmt::Write;
use core::ops::Add;
use crate::console::Console;
use crate::memlayout::CLINT_MTIME;
use crate::riscv::*;
use crate::param::*;
use crate::printf::{Printer, PRINTER};
use crate::proc::cpuid;

// ///////////////////////////////////
// / LANGUAGE STRUCTURES / FUNCTIONS
// ///////////////////////////////////
#[no_mangle]
extern "C" fn eh_personality() {}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    printf!("Aborting: \n");
    if let Some(p) = info.location() {
        printf!(
            "line {}, file {}: {}\n",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    }
    else {
        printf!("no information available.\n");
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

struct NoopAllocator{}
unsafe impl Sync for NoopAllocator {}
unsafe impl GlobalAlloc for NoopAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}
#[global_allocator]
static ALLOCATOR: NoopAllocator = NoopAllocator{};

#[no_mangle]
pub extern "C"
fn kmain() {
    if cpuid() == 0 {
        let mut console = Console::init();

        unsafe { PRINTER = Some(Printer::init(console)); }

        printf!("\n");
        printf!("xv6 kernel is booting\n");
        printf!("\n");
    }
}