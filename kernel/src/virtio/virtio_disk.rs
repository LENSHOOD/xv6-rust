//
// driver for qemu's virtio disk device.
// uses qemu's mmio interface to virtio.
//
// qemu ... -drive file=fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
//

use core::ptr;
use crate::buf::Buf;
use crate::debug_log;
use crate::kalloc::KMEM;
use crate::riscv::PGSIZE;
use crate::spinlock::Spinlock;
use crate::string::memset;
use crate::virtio::*;
// the address of virtio mmio register r.
macro_rules! Read_R {
    ( $r:expr ) => {
        unsafe {
            (($crate::memlayout::VIRTIO0 + $r) as *const usize).read_volatile() as u32
        }
    };
}

macro_rules! Write_R {
    ( $r:expr, $val:expr ) => {
        unsafe {
            (($crate::memlayout::VIRTIO0 + $r) as *mut usize).write_volatile($val as usize)
        }
    };
}

#[derive(Copy, Clone)]
struct Info<'a> {
    b: Option<&'a Buf>,
    status: u8,
}

struct Disk<'a> {
    // a set (not a ring) of DMA descriptors, with which the
    // driver tells the device where to read and write individual
    // disk operations. there are NUM descriptors.
    // most commands consist of a "chain" (a linked list) of a couple of
    // these descriptors.
    desc: *mut VirtqDesc,

    // a ring in which the driver writes descriptor numbers
    // that the driver would like the device to process.  it only
    // includes the head descriptor of each chain. the ring has
    // NUM elements.
    avail: *mut VirtqAvail,

    // a ring in which the device writes descriptor numbers that
    // the device has finished processing (just the head of each chain).
    // there are NUM used ring entries.
    used: *mut VirtqUsed,

    // our own book-keeping.
    free: [u8; NUM],  // is a descriptor free?
    used_idx: u16, // we've looked this far in used[2..NUM].

    // track info about in-flight operations,
    // for use when completion interrupt arrives.
    // indexed by first descriptor index of chain.
    info: [Info<'a>; NUM],

    // disk command headers.
    // one-for-one with descriptors, for convenience.
    ops: [VirtioBlkReq; NUM],

    vdisk_lock: Spinlock,

}

impl<'a> Disk<'a> {
    const fn create() -> Self {
        Self {
            desc: ptr::null_mut(),
            avail: ptr::null_mut(),
            used: ptr::null_mut(),
            free: [0; NUM],
            used_idx: 0,
            info: [Info{ b: None, status: 0 }; NUM],
            ops: [VirtioBlkReq{
                desc_type: 0,
                reserved: 0,
                sector: 0,
            }; NUM],
            vdisk_lock: Spinlock::init_lock("virtio_disk"),
        }
    }
}

static mut DISK: Disk = Disk::create();

pub fn virtio_disk_init() {
    if Read_R!(VIRTIO_MMIO_MAGIC_VALUE) != 0x74726976 ||
        Read_R!(VIRTIO_MMIO_VERSION) != 2 ||
        Read_R!(VIRTIO_MMIO_DEVICE_ID) != 2 ||
        Read_R!(VIRTIO_MMIO_VENDOR_ID) != 0x554d4551 {

        panic!("could not find virtio disk");
    }

    let mut status = 0;

    // reset device
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // set ACKNOWLEDGE status bit
    status |= VIRTIO_CONFIG_S_ACKNOWLEDGE;
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // set DRIVER status bit
    status |= VIRTIO_CONFIG_S_DRIVER;
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // negotiate features
    let mut features = Read_R!(VIRTIO_MMIO_DEVICE_FEATURES);
    features &= !(1 << VIRTIO_BLK_F_RO);
    features &= !(1 << VIRTIO_BLK_F_SCSI);
    features &= !(1 << VIRTIO_BLK_F_CONFIG_WCE);
    features &= !(1 << VIRTIO_BLK_F_MQ);
    features &= !(1 << VIRTIO_F_ANY_LAYOUT);
    features &= !(1 << VIRTIO_RING_F_EVENT_IDX);
    features &= !(1 << VIRTIO_RING_F_INDIRECT_DESC);
    Write_R!(VIRTIO_MMIO_DRIVER_FEATURES, features);

    // tell device that feature negotiation is complete.
    status |= VIRTIO_CONFIG_S_FEATURES_OK;
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // re-read status to ensure FEATURES_OK is set.
    status = Read_R!(VIRTIO_MMIO_STATUS) as usize;
    if !(status & VIRTIO_CONFIG_S_FEATURES_OK) == 0 {
        panic!("virtio disk FEATURES_OK unset");
    }

    // initialize queue 0.
    Write_R!(VIRTIO_MMIO_QUEUE_SEL, 0);

    // ensure queue 0 is not in use.
    if Read_R!(VIRTIO_MMIO_QUEUE_READY) != 0 {
        panic!("virtio disk should not be ready");
    }

    // check maximum queue size.
    let max = Read_R!(VIRTIO_MMIO_QUEUE_NUM_MAX);
    if max == 0 {
        panic!("virtio disk has no queue 0");
    }
    if (max as usize ) < NUM{
        panic!("virtio disk max queue too short");
    }

    // allocate and zero queue memory.
    unsafe {
        DISK.desc = KMEM.kalloc();
        DISK.avail = KMEM.kalloc();
        DISK.used = KMEM.kalloc();
        if DISK.desc.is_null() || DISK.avail.is_null() || DISK.used.is_null() {
            panic!("virtio disk kalloc");
        }
        memset(DISK.desc as *mut u8, 0, PGSIZE);
        memset(DISK.avail as *mut u8, 0, PGSIZE);
        memset(DISK.used as *mut u8, 0, PGSIZE);
    }

    // set queue size.
    Write_R!(VIRTIO_MMIO_QUEUE_NUM, NUM);

    // write physical addresses.
    Write_R!(VIRTIO_MMIO_QUEUE_DESC_LOW, DISK.desc.expose_addr());
    Write_R!(VIRTIO_MMIO_QUEUE_DESC_HIGH, DISK.desc.expose_addr() >> 32);
    Write_R!(VIRTIO_MMIO_DRIVER_DESC_LOW, DISK.avail.expose_addr());
    Write_R!(VIRTIO_MMIO_DRIVER_DESC_HIGH, DISK.avail.expose_addr() >> 32);
    Write_R!(VIRTIO_MMIO_DEVICE_DESC_LOW, DISK.used.expose_addr());
    Write_R!(VIRTIO_MMIO_DEVICE_DESC_HIGH, DISK.used.expose_addr() >> 32);

    // queue is ready.
    Write_R!(VIRTIO_MMIO_QUEUE_READY, 0x1);

    // all NUM descriptors start out unused.
    for i in 0..NUM {
        unsafe { DISK.free[i] = 1; }
    }

    // tell device we're completely ready.
    status |= VIRTIO_CONFIG_S_DRIVER_OK;
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // plic.c and trap.c arrange for interrupts from VIRTIO0_IRQ.
}

pub fn virtio_disk_rw(b: &Buf, write: bool) {
    panic!("unsupported")
}