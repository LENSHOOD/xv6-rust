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

use core::ptr::NonNull;
use crate::fs::BSIZE;
use crate::param::NBUF;
use crate::printf;
use crate::sleeplock::Sleeplock;
use crate::spinlock::Spinlock;

#[derive(Copy, Clone)]
struct Buf {
    valid: bool,   // has data been read from disk?
    disk: bool,    // does disk "own" buf?
    dev: u32,
    blockno: u32,
    lock: Sleeplock,
    refcnt: u32,
    prev: Option<NonNull<Buf>>, // LRU cache list
    next: Option<NonNull<Buf>>,
    data: [u8; BSIZE],
}

impl Buf {
    const fn new() -> Self {
        Buf {
            valid: false,
            disk: false,
            dev: 0,
            blockno: 0,
            lock: Sleeplock::init_lock("buffer"),
            refcnt: 0,
            prev: None,
            next: None,
            data: [0; BSIZE],
        }
    }
}
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
        let mut head_ptr = *(BCACHE.head).as_ptr();
        head_ptr.prev = Some(BCACHE.head);
        head_ptr.next = Some(BCACHE.head);
        for i in 0..NBUF {
            let mut b = &mut BCACHE.buf[i];
            b.next = head_ptr.next;
            b.prev = Some(BCACHE.head);

            let mut head_next = head_ptr.next.unwrap();
            (*head_next.as_ptr()).prev = NonNull::new(b as *mut Buf);
            head_ptr.next = NonNull::new(b as *mut Buf);
        }
    }
}
