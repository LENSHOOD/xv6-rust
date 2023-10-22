#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(const_mut_refs)]

extern crate alloc;

mod asm;
mod riscv;
mod memlayout;
mod param;
mod uart;
mod start;
mod spinlock;
mod sleeplock;
mod proc;
mod console;
mod printf;
mod kalloc;
mod string;
mod vm;
mod trap;
mod plic;
mod bio;
mod fs;
use core::alloc::{GlobalAlloc, Layout};
use crate::console::Console;
use crate::kalloc::{KMEM, KMem};
use crate::printf::{Printer, PRINTER};
use crate::proc::cpuid;

// ///////////////////////////////////
// / LANGUAGE STRUCTURES / FUNCTIONS
// ///////////////////////////////////
#[no_mangle]
extern "C" fn eh_personality() {}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    printf!("Aborting: \n");
    if let Some(p) = info.location() {
        printf!(
            "line {}, file {}: {}\n",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    }
    else {
        printf!("no information available.\n");
    }
    abort();
}

#[no_mangle]
extern "C"
fn abort() -> ! {
    loop {
        unsafe {
            core::arch::asm!("wfi")
        }
    }
}

struct NoopAllocator{}
unsafe impl Sync for NoopAllocator {}
unsafe impl GlobalAlloc for NoopAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        todo!()
    }
}
#[global_allocator]
static ALLOCATOR: NoopAllocator = NoopAllocator{};

#[no_mangle]
pub extern "C" fn kmain() {
    if cpuid() == 0 {
        let console = Console::init();
        unsafe { PRINTER = Some(Printer::init(console)); }
        printf!("\nxv6 kernel is booting...\n\n");
        unsafe { KMEM = Some(KMem::kinit()) } // physical page allocator
        // printf!("\nKernel memory initialized.\n\n");

        // debug info
        // unsafe {
        //     printf!("Ready to alloc\n");
        //     let first_page = KMEM.as_mut().unwrap().kalloc();
        //     printf!("First page starts at : 0x{:x}\n", first_page as usize);
        //     KMEM.as_mut().unwrap().kfree(first_page);
        //     printf!("Page freed\n");
        // }

        // printf!("Initializing virtual memory...\n");
        vm::kvminit(); // create kernel page table
        // printf!("{:?}", vm::KERNEL_PAGETABLE.unwrap());

        // debug_log!("Turn on paging...\n");
        vm::kvminithart(); // turn on paging
        debug_log!("Paging turned on.\n");

        debug_log!("Init processes...\n");
        proc::procinit(); // process table
        debug_log!("Processes initialized\n");

        debug_log!("Init trap...\n");
        trap::trapinit(); // trap vectors
        trap::trapinithart(); // install kernel trap vector
        debug_log!("Trap initialized\n");

        debug_log!("Init plic...\n");
        plic::plicinit(); // set up interrupt controller
        plic::plicinithart(); // ask PLIC for device interrupts
        debug_log!("Plic initialized\n");

        debug_log!("Init buffer cache...\n");
        bio::binit(); // // buffer cache
        debug_log!("Buffer cache initialized\n");

        printf!("System boot successful")
    }
}