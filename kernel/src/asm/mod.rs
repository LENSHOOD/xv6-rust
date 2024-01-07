use core::arch::global_asm;
use crate::memlayout::TRAPFRAME;

global_asm!(include_str!("trampoline.S"));
global_asm!(include_str!("entry.S"));
global_asm!(include_str!("kernelvec.S"));
global_asm!(include_str!("switch.S"));
