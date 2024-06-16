pub mod virtio_disk;

//
// virtio device definitions.
// for both the mmio interface, and virtio descriptors.
// only tested with qemu.
//
// the virtio spec:
// https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.pdf
//

// virtio mmio control registers, mapped starting at 0x10001000.
// from qemu virtio_mmio.h
const VIRTIO_MMIO_MAGIC_VALUE: usize = 0x000; // 0x74726976
const VIRTIO_MMIO_VERSION: usize = 0x004; // version; should be 2
const VIRTIO_MMIO_DEVICE_ID: usize = 0x008; // device type; 1 is net, 2 is disk
const VIRTIO_MMIO_VENDOR_ID: usize = 0x00c; // 0x554d4551
const VIRTIO_MMIO_DEVICE_FEATURES: usize = 0x010;
const VIRTIO_MMIO_DRIVER_FEATURES: usize = 0x020;
const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030; // select queue, write-only
const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034; // max size of current queue, read-only
const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038; // size of current queue, write-only
const VIRTIO_MMIO_QUEUE_READY: usize = 0x044; // ready bit
const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050; // write-only
const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060; // read-only
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064; // write-only
const VIRTIO_MMIO_STATUS: usize = 0x070; // read/write
const VIRTIO_MMIO_QUEUE_DESC_LOW: usize = 0x080; // physical address for descriptor table, write-only
const VIRTIO_MMIO_QUEUE_DESC_HIGH: usize = 0x084;
const VIRTIO_MMIO_DRIVER_DESC_LOW: usize = 0x090; // physical address for available ring, write-only
const VIRTIO_MMIO_DRIVER_DESC_HIGH: usize = 0x094;
const VIRTIO_MMIO_DEVICE_DESC_LOW: usize = 0x0a0; // physical address for used ring, write-only
const VIRTIO_MMIO_DEVICE_DESC_HIGH: usize = 0x0a4;

// status register bits, from qemu virtio_config.h
const VIRTIO_CONFIG_S_ACKNOWLEDGE: usize = 1;
const VIRTIO_CONFIG_S_DRIVER: usize = 2;
const VIRTIO_CONFIG_S_DRIVER_OK: usize = 4;
const VIRTIO_CONFIG_S_FEATURES_OK: usize = 8;

// device feature bits
const VIRTIO_BLK_F_RO: usize = 5; /* Disk is read-only */
const VIRTIO_BLK_F_SCSI: usize = 7; /* Supports scsi command passthru */
const VIRTIO_BLK_F_CONFIG_WCE: usize = 11; /* Writeback mode available in config */
const VIRTIO_BLK_F_MQ: usize = 12; /* support more than one vq */
const VIRTIO_F_ANY_LAYOUT: usize = 27;
const VIRTIO_RING_F_INDIRECT_DESC: usize = 28;
const VIRTIO_RING_F_EVENT_IDX: usize = 29;

// this many virtio descriptors.
// must be a power of two.
const NUM: usize = 8;

// a single descriptor, from the spec.
#[derive(Copy, Clone)]
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}
const VRING_DESC_F_NEXT: u16 = 1; // chained with another descriptor
const VRING_DESC_F_WRITE: u16 = 2; // device writes (vs read)

// the (entire) avail ring, from the spec.
#[derive(Copy, Clone)]
#[repr(C)]
struct VirtqAvail {
    flags: u16,       // always zero
    idx: u16,         // driver will write ring[idx] next
    ring: [u16; NUM], // descriptor numbers of chain heads
    unused: u16,
}

// one entry in the "used" ring, with which the
// device tells the driver about completed requests.
#[derive(Copy, Clone)]
#[repr(C)]
struct VirtqUsedElem {
    id: u32, // index of start of completed descriptor chain
    len: u32,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct VirtqUsed {
    flags: u16, // always zero
    idx: u16,   // device increments when it adds a ring[] entry
    ring: [VirtqUsedElem; NUM],
}

// these are specific to virtio block devices, e.g. disks,
// described in Section 5.2 of the spec.

const VIRTIO_BLK_T_IN: u32 = 0; // read the disk
const VIRTIO_BLK_T_OUT: u32 = 1; // write the disk

// the format of the first descriptor in a disk request.
// to be followed by two more descriptors containing
// the block, and a one-byte status.
#[derive(Copy, Clone)]
#[repr(C)]
struct VirtioBlkReq {
    desc_type: u32, // VIRTIO_BLK_T_IN or ..._OUT
    reserved: u32,
    sector: u64,
}
