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
mod buf;
mod bio;
mod fs;
mod file;
mod pipe;
mod stat;
mod virtio;

mod log;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::kalloc::KMem;
use crate::printf::Printer;
use crate::proc::cpuid;
use crate::riscv::__sync_synchronize;

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
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        todo!()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        todo!()
    }
}
#[global_allocator]
static ALLOCATOR: NoopAllocator = NoopAllocator{};

static STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "C" fn kmain() {
    if cpuid() == 0 {
        Printer::init();
        printf!("\nxv6 kernel is booting...\n\n");

        KMem::kinit(); // physical page allocator
        debug_log!("Kernel memory initialized.\n");

        // debug info
        // unsafe {
        //     printf!("Ready to alloc\n");
        //     let first_page = KMEM.as_mut().unwrap().kalloc();
        //     printf!("First page starts at : 0x{:x}\n", first_page as usize);
        //     KMEM.as_mut().unwrap().kfree(first_page);
        //     printf!("Page freed\n");
        // }

        vm::kvminit(); // create kernel page table
        // debug_log!("{:?}", vm::KERNEL_PAGETABLE.unwrap());
        debug_log!("Virtual memory initialized.\n");

        vm::kvminithart(); // turn on paging
        debug_log!("Paging turned on.\n");

        proc::procinit(); // process table
        debug_log!("Processes initialized\n");

        trap::trapinit(); // trap vectors
        trap::trapinithart(); // install kernel trap vector
        debug_log!("Trap initialized\n");

        plic::plicinit(); // set up interrupt controller
        plic::plicinithart(); // ask PLIC for device interrupts
        debug_log!("Plic initialized\n");

        bio::binit(); // buffer cache
        debug_log!("Buffer cache initialized\n");

        fs::fs::iinit(); // inode table
        file::file::fileinit(); // file table
        debug_log!("ITable FTable initialized\n");

        virtio::virtio_disk::virtio_disk_init(); // emulated hard disk
        debug_log!("VirtIO disk initialized\n");

        proc::userinit(); // first user process
        debug_log!("First user process initialized\n");

        __sync_synchronize();
        STARTED.store(true, Ordering::Relaxed);
        printf!("\nSystem boot successful\n")
    } else {
        while !STARTED.load(Ordering::Relaxed) {}

        __sync_synchronize();
        printf!("hart {} starting\n", cpuid());
        vm::kvminithart();    // turn on paging
        trap::trapinithart();   // install kernel trap vector
        plic::plicinithart();    // ask PLIC for device interrupts
    }

    proc::scheduler();
}