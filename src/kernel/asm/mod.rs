use core::arch::global_asm;

global_asm!(include_str!("trampoline.S"));
global_asm!(include_str!("entry.S"));
global_asm!(include_str!("kernelvec.S"));
