use core::arch::global_asm;

global_asm!(include_str!("kernel/trampoline.S"));
global_asm!(include_str!("kernel/entry.S"));
global_asm!(include_str!("kernel/kernelvec.S"));
