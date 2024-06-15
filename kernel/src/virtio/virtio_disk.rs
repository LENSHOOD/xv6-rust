//
// driver for qemu's virtio disk device.
// uses qemu's mmio interface to virtio.
//
// qemu ... -drive file=fs.img,if=none,format=raw,id=x0 -device virtio-blk-device,drive=x0,bus=virtio-mmio-bus.0
//

use crate::buf::Buf;
use crate::fs::BSIZE;
use crate::kalloc::KMEM;
use crate::proc::{sleep, wakeup};
use crate::riscv::{__sync_synchronize, PGSIZE};
use crate::spinlock::Spinlock;
use crate::string::memset;
use crate::virtio::*;
use core::{mem, ptr};
// the address of virtio mmio register r.
macro_rules! Read_R {
    ( $r:expr ) => {
        unsafe { (($crate::memlayout::VIRTIO0 + $r) as *const usize).read_volatile() as u32 }
    };
}

macro_rules! Write_R {
    ( $r:expr, $val:expr ) => {
        unsafe { (($crate::memlayout::VIRTIO0 + $r) as *mut usize).write_volatile($val as usize) }
    };
}

#[derive(Copy, Clone)]
struct Info {
    b: Option<*mut Buf>,
    status: u8,
}

struct Disk {
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
    free: [bool; NUM], // is a descriptor free?
    used_idx: u16,     // we've looked this far in used[2..NUM].

    // track info about in-flight operations,
    // for use when completion interrupt arrives.
    // indexed by first descriptor index of chain.
    info: [Info; NUM],

    // disk command headers.
    // one-for-one with descriptors, for convenience.
    ops: [VirtioBlkReq; NUM],

    vdisk_lock: Spinlock,
}

impl Disk {
    const fn create() -> Self {
        Self {
            desc: ptr::null_mut(),
            avail: ptr::null_mut(),
            used: ptr::null_mut(),
            free: [false; NUM],
            used_idx: 0,
            info: [Info { b: None, status: 0 }; NUM],
            ops: [VirtioBlkReq {
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
    if Read_R!(VIRTIO_MMIO_MAGIC_VALUE) != 0x74726976
        || Read_R!(VIRTIO_MMIO_VERSION) != 2
        || Read_R!(VIRTIO_MMIO_DEVICE_ID) != 2
        || Read_R!(VIRTIO_MMIO_VENDOR_ID) != 0x554d4551
    {
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
    if (max as usize) < NUM {
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
        unsafe {
            DISK.free[i] = true;
        }
    }

    // tell device we're completely ready.
    status |= VIRTIO_CONFIG_S_DRIVER_OK;
    Write_R!(VIRTIO_MMIO_STATUS, status);

    // plic.c and trap.c arrange for interrupts from VIRTIO0_IRQ.
}

pub unsafe fn virtio_disk_rw(b: &mut Buf, write: bool) {
    DISK.vdisk_lock.acquire();

    // the spec's Section 5.2 says that legacy block operations use
    // three descriptors: one for type/reserved/sector, one for the
    // data, one for a 1-byte status result.

    // allocate the three descriptors.
    let idx = loop {
        match alloc3_desc() {
            None => sleep(&DISK.free as *const [bool; NUM], &mut DISK.vdisk_lock),
            Some(idx) => break idx,
        }
    };

    // format the three descriptors.
    // qemu's virtio-blk.c reads them.

    let sector = (b.blockno * (BSIZE / 512) as u32) as u64;
    let buf0 = &mut DISK.ops[idx[0]];

    if write {
        buf0.desc_type = VIRTIO_BLK_T_OUT; // write the disk
    } else {
        buf0.desc_type = VIRTIO_BLK_T_IN; // read the disk
    }
    buf0.reserved = 0;
    buf0.sector = sector;

    let virt_desc_0 = DISK.desc.add(idx[0]).as_mut().unwrap();
    virt_desc_0.addr = (buf0 as *mut VirtioBlkReq).expose_addr() as u64;
    virt_desc_0.len = mem::size_of::<VirtioBlkReq>() as u32;
    virt_desc_0.flags = VRING_DESC_F_NEXT;
    virt_desc_0.next = idx[1] as u16;

    let virt_desc_1 = DISK.desc.add(idx[1]).as_mut().unwrap();
    virt_desc_1.addr = (&b.data as *const u8).expose_addr() as u64;
    virt_desc_1.len = BSIZE as u32;
    if write {
        virt_desc_1.flags = 0; // device reads b->data
    } else {
        virt_desc_1.flags = VRING_DESC_F_WRITE; // device writes b->data
    }
    virt_desc_1.flags |= VRING_DESC_F_NEXT;
    virt_desc_1.next = idx[2] as u16;

    DISK.info[idx[0]].status = 0xff; // device writes 0 on success

    let virt_desc_2 = DISK.desc.add(idx[2]).as_mut().unwrap();
    virt_desc_2.addr = (&DISK.info[idx[0]].status as *const u8).expose_addr() as u64;
    virt_desc_2.len = 1;
    virt_desc_2.flags = VRING_DESC_F_WRITE; // device writes the status
    virt_desc_2.next = 0;

    // record struct buf for virtio_disk_intr().
    b.disk = true;
    DISK.info[idx[0]].b = Some(b);

    // tell the device the first index in our chain of descriptors.
    let avail = DISK.avail.as_mut().unwrap();
    avail.ring[avail.idx as usize % NUM] = idx[0] as u16;

    __sync_synchronize();

    // tell the device another avail ring entry is available.
    avail.idx += 1; // not % NUM ...

    __sync_synchronize();

    Write_R!(VIRTIO_MMIO_QUEUE_NOTIFY, 0); // value is queue number

    // Wait for virtio_disk_intr() to say request has finished.
    while b.disk == true {
        sleep(b as *const Buf, &mut DISK.vdisk_lock);
    }

    DISK.info[idx[0]].b = None;
    free_chain(idx[0]);

    DISK.vdisk_lock.release();
}

// allocate three descriptors (they need not be contiguous).
// disk transfers always use three descriptors.
fn alloc3_desc() -> Option<[usize; 3]> {
    let mut idx = [0; 3];
    for i in 0..3 {
        unsafe {
            match alloc_desc() {
                None => {
                    for j in 0..i {
                        free_desc(idx[j]);
                    }
                    return None;
                }
                Some(curr) => idx[i] = curr,
            }
        }
    }

    Some(idx)
}

// find a free descriptor, mark it non-free, return its index.
unsafe fn alloc_desc() -> Option<usize> {
    for i in 0..NUM {
        if DISK.free[i] {
            DISK.free[i] = false;
            return Some(i);
        }
    }

    None
}

// mark a descriptor as free.
unsafe fn free_desc(i: usize) {
    if i >= NUM {
        panic!("free_desc 1");
    }

    if DISK.free[i] {
        panic!("free_desc 2");
    }

    let desc = DISK.desc.add(i).as_mut().unwrap();
    desc.addr = 0;
    desc.len = 0;
    desc.flags = 0;
    desc.next = 0;
    DISK.free[i] = true;
    wakeup(&DISK.free[0]);
}

// free a chain of descriptors.
unsafe fn free_chain(i: usize) {
    let mut i = i;
    loop {
        let desc = DISK.desc.add(i).as_mut().unwrap();
        let flag = desc.flags;
        let nxt = desc.next;
        free_desc(i);
        if flag & VRING_DESC_F_NEXT != 0 {
            break;
        }

        i = nxt as usize;
    }
}

pub(crate) unsafe fn virtio_disk_intr() {
    DISK.vdisk_lock.acquire();

    // the device won't raise another interrupt until we tell it
    // we've seen this interrupt, which the following line does.
    // this may race with the device writing new entries to
    // the "used" ring, in which case we may process the new
    // completion entries in this interrupt, and have nothing to do
    // in the next interrupt, which is harmless.
    Write_R!(
        VIRTIO_MMIO_INTERRUPT_ACK,
        Read_R!(VIRTIO_MMIO_INTERRUPT_STATUS) & 0x3
    );

    __sync_synchronize();

    // the device increments disk.used->idx when it
    // adds an entry to the used ring.

    while DISK.used_idx != DISK.used.as_mut().unwrap().idx {
        __sync_synchronize();
        let id = DISK.used.as_mut().unwrap().ring[DISK.used_idx as usize % NUM].id as usize;

        if DISK.info[id].status != 0 {
            panic!("virtio_disk_intr status");
        }

        let b = DISK.info[id].b.unwrap().as_mut().unwrap();
        b.disk = false; // disk is done with buf
        wakeup(b);

        DISK.used_idx += 1;
    }

    DISK.vdisk_lock.release();
}
