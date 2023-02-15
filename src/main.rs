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

use core::alloc::{GlobalAlloc, Layout};
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
    let mut my_uart = uart::UartDriver::new(0x1000_0000);
    my_uart.init();

    println!("This is my operating system!");
    println!("I'm so awesome. If you start typing something, I'll show you what you typed!");
}