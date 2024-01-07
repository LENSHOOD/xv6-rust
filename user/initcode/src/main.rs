use core::arch::global_asm;

extern crate kernel;

global_asm!(include_str!("initcode.S"));

fn main() {
    
}