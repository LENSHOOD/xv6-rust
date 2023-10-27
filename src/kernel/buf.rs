use core::ptr::NonNull;
use crate::fs::BSIZE;
use crate::sleeplock::Sleeplock;

#[derive(Copy, Clone)]
pub struct Buf {
    valid: bool,   // has data been read from disk?
    disk: bool,    // does disk "own" buf?
    dev: u32,
    blockno: u32,
    lock: Sleeplock,
    refcnt: u32,
    pub(crate) prev: Option<NonNull<Buf>>, // LRU cache list
    pub(crate) next: Option<NonNull<Buf>>,
    data: [u8; BSIZE],
}

impl Buf {
    pub const fn new() -> Self {
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