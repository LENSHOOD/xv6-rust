#![no_std]
#![feature(start)]

use ulib::printf;

#[start]
fn main(_argc: isize, _argv: *const *const u8) -> isize {
    printf!("test init!!!");

    0
}
