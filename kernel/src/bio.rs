// Buffer cache.
//
// The buffer cache is a linked list of buf structures holding
// cached copies of disk block contents.  Caching disk blocks
// in memory reduces the number of disk reads and also provides
// a synchronization point for disk blocks used by multiple processes.
//
// Interface:
// * To get a buffer for a particular disk block, call bread.
// * After changing buffer data, call bwrite to write it to disk.
// * When done with the buffer, call brelse.
// * Do not use the buffer after calling brelse.
// * Only one process at a time can use a buffer,
//     so do not keep them longer than necessary.

use crate::buf::Buf;
use crate::param::NBUF;
use crate::spinlock::Spinlock;
use crate::virtio::virtio_disk::virtio_disk_rw;
use core::ptr::NonNull;

struct BCache {
    lock: Spinlock,
    buf: [Buf; NBUF],

    // Linked list of all buffers, through prev/next.
    // Sorted by how recently the buffer was used.
    // head.next is most recent, head.prev is least.
    head: NonNull<Buf>,
}

static mut DUMMY_HEAD: Buf = Buf::new();
static mut BCACHE: BCache = BCache {
    lock: Spinlock::init_lock("bcache"),
    buf: [Buf::new(); NBUF],
    head: unsafe { NonNull::new_unchecked((&mut DUMMY_HEAD) as *mut Buf) },
};

pub fn binit() {
    unsafe {
        // At here, some interesting things may happen, if not handled carefully:
        // We know that, the Buf contains a "huge" field buf.data, which type is [u8; BSIZE(1024)],
        // thus consume 1 KiB mem space. And the BCache contains NBUF(30) numbers of that.
        //
        // The 30*1 KiB mem space is quite big. If we carelessly initialize the BCache in this
        // "binit()" function like this:
        //
        //         let mut b_cache = BCache {
        //             lock: Spinlock::init_lock("bcache"),
        //             buf: [Buf::new(); NBUF],
        //             head: NonNull::new_unchecked((&mut Buf::new()) as *mut Buf),
        //         };
        //         BCACHE = Some(b_cache)
        //
        // rather than directly initialized it at the static field, then the stack space would be blew up. ^_^
        // (we only have 4096-bytes kernel stack per CPU, see entry.S)

        // Create linked list of buffers
        let mut head_ptr = *BCACHE.head.as_ptr();
        head_ptr.prev = Some(BCACHE.head);
        head_ptr.next = Some(BCACHE.head);
        for b in &mut BCACHE.buf {
            b.next = head_ptr.next;
            b.prev = Some(BCACHE.head);

            let head_next = head_ptr.next.unwrap();
            (*head_next.as_ptr()).prev = NonNull::new(b as *mut Buf);
            head_ptr.next = NonNull::new(b as *mut Buf);
        }
    }
}

// Look through buffer cache for block on device dev.
// If not found, allocate a buffer.
// In either case, return locked buffer.
fn bget(dev: u32, blockno: u32) -> &'static mut Buf {
    unsafe {
        BCACHE.lock.acquire();

        // Is the block already cached?
        loop {
            let head_ptr = BCACHE.head.as_ptr();
            let head = *head_ptr;
            let b_ptr = head.next.unwrap().as_ptr();
            if b_ptr == head_ptr {
                break;
            }

            let mut b = *b_ptr;
            if b.dev == dev && b.blockno == blockno {
                b.refcnt += 1;
                BCACHE.lock.release();
                b.lock.acquire_sleep();
                return &mut *head.next.unwrap().as_ptr();
            }

            b = *b.next.unwrap().as_ptr();
        }

        // Not cached.
        // Recycle the least recently used (LRU) unused buffer.
        loop {
            let head_ptr = BCACHE.head.as_ptr();
            let head = *head_ptr;
            let b_ptr = head.prev.unwrap().as_ptr();
            if b_ptr == head_ptr {
                break;
            }

            let mut b = *b_ptr;
            if b.refcnt == 0 {
                b.dev = dev;
                b.blockno = blockno;
                b.valid = false;
                b.refcnt = 1;
                BCACHE.lock.release();
                b.lock.acquire_sleep();
                return &mut *head.prev.unwrap().as_ptr();
            }

            b = *b.prev.unwrap().as_ptr();
        }
    }

    panic!("bget: no buffers");
}

// Return a locked buf with the contents of the indicated block.
pub fn bread(dev: u32, blockno: u32) -> &'static mut Buf {
    let b = bget(dev, blockno);
    if !b.valid {
        unsafe { virtio_disk_rw(b as *mut Buf, false) };
        b.valid = true
    }

    return b;
}

// Write b's contents to disk.  Must be locked.
pub fn bwrite(b: *mut Buf) {
    if !unsafe { b.as_mut() }.unwrap().lock.holding_sleep() {
        panic!("bwrite");
    }
    unsafe {
        virtio_disk_rw(b, true);
    }
}

// Release a locked buffer.
// Move to the head of the most-recently-used list.
pub fn brelse(b: &mut Buf) {
    if !b.lock.holding_sleep() {
        panic!("brelse");
    }

    b.lock.release_sleep();
    unsafe {
        BCACHE.lock.acquire();
        b.refcnt -= 1;
        if b.refcnt == 0 {
            (*b.next.unwrap().as_ptr()).prev = b.prev;
            (*b.prev.unwrap().as_ptr()).next = b.next;

            let mut head = *BCACHE.head.as_ptr();
            b.next = head.next;
            b.prev = Some(BCACHE.head);

            let b = NonNull::new_unchecked(b as *mut Buf);
            (*head.next.unwrap().as_ptr()).prev = Some(b);
            head.next = Some(b);
        }

        BCACHE.lock.release();
    }
}

pub fn bpin(b: &mut Buf) {
    unsafe {
        BCACHE.lock.acquire();
        b.refcnt += 1;
        BCACHE.lock.release()
    }
}

pub fn bunpin(b: *mut Buf) {
    unsafe {
        BCACHE.lock.acquire();
        b.as_mut().unwrap().refcnt -= 1;
        BCACHE.lock.release()
    }
}
