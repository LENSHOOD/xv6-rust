#![no_std]
#![feature(start)]

extern crate ulib;

use ulib::printf;

#[start]
fn main(_argc: isize, _argv: *const *const u8) -> isize {
    printf!("test init!!!");

    0
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}