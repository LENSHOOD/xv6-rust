// Physical memory layout

// qemu -machine virt is set up like this,
// based on qemu's hw/riscv/virt.c:
//
// 00001000 -- boot ROM, provided by qemu
// 02000000 -- CLINT
// 0C000000 -- PLIC
// 10000000 -- uart0
// 10001000 -- virtio disk
// 80000000 -- boot ROM jumps here in machine mode
//             -kernel loads the kernel here
// unused RAM after 80000000.

// the kernel uses physical memory thus:
// 80000000 -- entry.S, then kernel text and data
// end -- start of kernel page allocation area
// PHYSTOP -- end RAM used by the kernel

use crate::riscv::*;

// qemu puts UART registers here in physical memory.
pub const UART0: usize = 0x10000000;
pub const UART0_IRQ: usize = 10;

// virtio mmio interface
pub const VIRTIO0: usize = 0x10001000;
pub const VIRTIO0_IRQ: usize = 1;

// core local interruptor (CLINT), which contains the timer.
pub const CLINT: u64 = 0x2000000;
#[macro_export]
macro_rules! CLINT_MTIMECMP {
    ( $hartid:expr ) => {
        $crate::memlayout::CLINT + 0x4000 + 8 * ($hartid)
    };
}
pub const CLINT_MTIME: u64 = CLINT + 0xBFF8; // cycles since boot.

// qemu puts platform-level interrupt controller (PLIC) here.
pub const PLIC: usize = 0x0c000000;
pub const PLIC_PRIORITY: usize = PLIC + 0x0;
pub const PLIC_PENDING: usize = PLIC + 0x1000;
#[macro_export]
macro_rules! PLIC_MENABLE {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x2000 + ($hart) * 0x100
    };
}
#[macro_export]
macro_rules! PLIC_SENABLE {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x2080 + ($hart) * 0x100
    };
}
#[macro_export]
macro_rules! PLIC_MPRIORITY {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x200000 + ($hart) * 0x2000
    };
}
#[macro_export]
macro_rules! PLIC_SPRIORITY {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x201000 + ($hart) * 0x2000
    };
}
#[macro_export]
macro_rules! PLIC_MCLAIM {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x200004 + ($hart) * 0x2000
    };
}
#[macro_export]
macro_rules! PLIC_SCLAIM {
    ( $hart:expr ) => {
        $crate::memlayout::PLIC + 0x201004 + ($hart) * 0x2000
    };
}

// the kernel expects there to be RAM
// for use by the kernel and user pages
// from physical address 0x80000000 to PHYSTOP.
pub const KERNBASE: usize = 0x80000000;
pub const PHYSTOP: usize = KERNBASE + 128 * 1024 * 1024;

// map the trampoline page to the highest address,
// in both user and kernel space.
pub const TRAMPOLINE: usize = MAXVA - PGSIZE;

// map kernel stacks beneath the trampoline,
// each surrounded by invalid guard pages.
// --------
// In our rust approach, it's quite easy to exhaust the one-page kernel stack,
// because we would intentionally or not to use the rust core lib to simplify
// the code.
// However once the sp register exceeds the process stack boundary
// and point to an illegal virtual address, then the risc-v will trap into an
// exception, with scause = 0xf (Store/AMO page fault) and store the illegal
// address into the stval.
// Therefore, we extend the kernel stack to two-pages, and still, with one guard
// page. Then each process will hold three pages in total for their kernel stack.
#[macro_export]
macro_rules! KSTACK {
    ( $p:expr ) => {
        $crate::memlayout::TRAMPOLINE - (($p) + 1) * 3 * $crate::riscv::PGSIZE
    };
}

// User memory layout.
// Address zero first:
//   text
//   original data and bss
//   fixed-size stack
//   expandable heap
//   ...
//   TRAPFRAME (p->trapframe, used by the trampoline)
//   TRAMPOLINE (the same page as in the kernel)
pub const TRAPFRAME: usize = TRAMPOLINE - PGSIZE;
