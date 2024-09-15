#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(strict_provenance)]
#![feature(const_mut_refs)]

extern crate alloc;

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::console::Console;
use crate::kalloc::KMem;
use crate::proc::cpuid;
use crate::riscv::__sync_synchronize;
use crate::uart::Uart;

mod asm;
mod bio;
mod buf;
mod console;
mod elf;
mod exec;
pub mod file;
mod fs;
mod kalloc;
mod log;
mod memlayout;
mod param;
mod pipe;
mod plic;
mod printf;
mod proc;
mod riscv;
mod sleeplock;
mod spinlock;
mod start;
mod stat;
pub mod string;
pub mod syscall;
mod trap;
mod uart;
mod virtio;
mod vm;

// ///////////////////////////////////
// / LANGUAGE STRUCTURES / FUNCTIONS
// ///////////////////////////////////
#[no_mangle]
extern "C" fn eh_personality() {}

pub(crate) static PANICKED: AtomicBool = AtomicBool::new(false);
#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    printf!("Aborting: \n");
    if let Some(p) = info.location() {
        printf!(
            "line {}, file {}: {}\n",
            p.line(),
            p.file(),
            info.message().unwrap()
        );
    } else {
        printf!("no information available.\n");
    }

    PANICKED.store(true, Ordering::Relaxed);
    abort();
}

#[no_mangle]
extern "C" fn abort() -> ! {
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}

struct NoopAllocator {}
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
static ALLOCATOR: NoopAllocator = NoopAllocator {};

static STARTED: AtomicBool = AtomicBool::new(false);

#[no_mangle]
pub extern "C" fn kmain() {
    if cpuid() == 0 {
        Uart::init();
        Console::init();
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
        vm::kvminithart(); // turn on paging
        trap::trapinithart(); // install kernel trap vector
        plic::plicinithart(); // ask PLIC for device interrupts
    }

    printf!("\nCPU {} start scheduling\n", cpuid());
    proc::scheduler();
}
