#![no_std]
#![feature(start)]

use kernel::string::strlen;
use ulib::stubs::{exit, write};

#[start]
fn main(argc: isize, argv: *const *const u8) -> isize {
    unsafe {
        for i in 1..argc {
            let curr_argv = argv.add(i as usize).read_volatile();
            let sz = strlen(curr_argv);
            write(1, curr_argv, sz as i32);
            if i + 1 < argc {
                write(1, &(' ' as u8) as *const u8, 1);
            } else {
                write(1, &('\n' as u8) as *const u8, 1);
            }
        }

        exit(0);
    }
}
