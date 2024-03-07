#![no_std]
mod stubs;

use core::fmt::Arguments;
use core::fmt::{Write, Error};
use core::result::{Result, Result::Ok};
use core::arch::global_asm;
use crate::stubs::write;

global_asm!(include_str!("usys.S"));

#[macro_export]
macro_rules! printf
{
	($($arg:tt)*) => {
        unsafe {
            crate::printf(core::format_args!($($arg)*))
        }
    };
}

struct Printer(i32);
impl Write for Printer {
    // The trait Write expects us to write the function write_str
    // which looks like:
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            unsafe { write(self.0, &c as *const u8, 1); }
        }
        // Return that we succeeded.
        Ok(())
    }
}

pub(crate) fn fprintf(fd: i32, args: Arguments<'_>) {
    Printer(fd).write_fmt(args).unwrap();
}

pub(crate) fn printf(args: Arguments<'_>) {
    fprintf(1, args);
}