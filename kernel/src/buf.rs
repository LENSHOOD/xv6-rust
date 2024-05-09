use crate::fs::BSIZE;
use crate::sleeplock::Sleeplock;
use core::ptr::NonNull;

#[derive(Copy, Clone)]
pub struct Buf {
    pub(crate) valid: bool, // has data been read from disk?
    pub(crate) disk: bool,  // does disk "own" buf?
    pub(crate) dev: u32,
    pub(crate) blockno: u32,
    pub(crate) lock: Sleeplock,
    pub(crate) refcnt: u32,
    pub(crate) prev: Option<NonNull<Buf>>, // LRU cache list
    pub(crate) next: Option<NonNull<Buf>>,
    pub(crate) data: [u8; BSIZE],
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
