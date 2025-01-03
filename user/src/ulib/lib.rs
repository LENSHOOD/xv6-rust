#![feature(strict_provenance)]
#![no_std]

use core::arch::global_asm;
use core::fmt::Arguments;
use core::fmt::{Error, Write};
use core::result::{Result, Result::Ok};

use crate::stubs::{read, write, exit};

pub mod stubs;
mod umalloc;

global_asm!(include_str!("usys.S"));

#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(p) = info.location() {
        printf!(
            "line {}, file {}: {}\n",
            p.line(),
            p.file(),
            info.message()
        );
    } else {
        printf!("no information available.\n");
    }

    unsafe { exit(-1); }
}

#[macro_export]
macro_rules! printf
{
	($($arg:tt)*) => {
        unsafe {
            printf(core::format_args!($($arg)*))
        }
    };
}

#[macro_export]
macro_rules! fprintf
{
	($fd:expr, $($arg:tt)*) => {
        unsafe {
            fprintf($fd, core::format_args!($($arg)*))
        }
    };
}

struct Printer(i32);
impl Write for Printer {
    // The trait Write expects us to write the function write_str
    // which looks like:
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            unsafe {
                write(self.0, &c as *const u8, 1);
            }
        }
        // Return that we succeeded.
        Ok(())
    }
}

pub fn fprintf(fd: i32, args: Arguments<'_>) {
    Printer(fd).write_fmt(args).unwrap();
}

pub fn printf(args: Arguments<'_>) {
    fprintf(1, args);
}

pub fn gets(buf: &mut [u8], max: usize) {
    let mut c: u8 = 0;
    let mut i = 0;
    while i + 1 < max {
        let cc = unsafe { read(0, &mut c as *mut u8, 1) };
        if cc < 1 {
            break;
        }

        unsafe {
            buf[i] = c;
        }
        i += 1;
        if c == b'\n' || c == b'\r' {
            break;
        }
    }

    unsafe {
        buf[i] = b'\0';
    }
}

pub fn strchr(s: &[u8], c: u8) -> u8 {
    for i in 0..s.len() {
        if s[i] == c {
            return s[i];
        }
    }

    0
}
