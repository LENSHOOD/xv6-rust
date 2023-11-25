use core::mem;
use crate::bio::{bpin, bread, brelse, bunpin, bwrite};
use crate::buf::Buf;
use crate::fs::{BSIZE, SuperBlock};
use crate::param::LOGSIZE;
use crate::spinlock::Spinlock;

// Simple logging that allows concurrent FS system calls.
//
// A log transaction contains the updates of multiple FS system
// calls. The logging system only commits when there are
// no FS system calls active. Thus there is never
// any reasoning required about whether a commit might
// write an uncommitted system call's updates to disk.
//
// A system call should call begin_op()/end_op() to mark
// its start and end. Usually begin_op() just increments
// the count of in-progress FS system calls and returns.
// But if it thinks the log is close to running out, it
// sleeps until the last outstanding end_op() commits.
//
// The log is a physical re-do log containing disk blocks.
// The on-disk log format:
//   header block, containing block #s for block A, B, C, ...
//   block A
//   block B
//   block C
//   ...
// Log appends are synchronous.

// Contents of the header block, used for both the on-disk header block
// and to keep track in memory of logged block# before commit.
struct LogHeader {
    n: u32,
    block: [u32; LOGSIZE],
}

struct Log {
    lock: Spinlock,
    start: u32,
    size: u32,
    outstanding: u32, // how many FS sys calls are executing.
    committing: i32,  // in commit(), please wait.
    dev: u32,
    lh: LogHeader,
}

static mut LOG: Log = Log {
    lock: Spinlock::init_lock("log"),
    start: 0,
    size: 0,
    outstanding: 0,
    committing: 0,
    dev: 0,
    lh: LogHeader { n: 0, block: [0; LOGSIZE] },
};

pub fn initlog(dev: u32, sb: &SuperBlock) {
    if mem::size_of::<LogHeader>() >= BSIZE {
        panic!("initlog: too big logheader");
    }

    unsafe {
        LOG.start = sb.logstart;
        LOG.size = sb.nlog;
        LOG.dev = dev;
        recover_from_log();
    }
}

unsafe fn recover_from_log() {
    read_head();
    install_trans(true); // if committed, copy from log to disk
    unsafe { LOG.lh.n = 0; }
    write_head(); // clear the log
}

// Read the log header from disk into the in-memory log header
unsafe fn read_head() {
    let buf = bread(LOG.dev, LOG.start);
    let (_head, body, _tail) = buf.data[0..mem::size_of::<LogHeader>()].align_to::<LogHeader>();
    let lh = &body[0];
    LOG.lh.n = lh.n;
    for i in 0..LOG.lh.n as usize {
        LOG.lh.block[i] = lh.block[i];
    }
    brelse(buf);
}

// Copy committed blocks from log to their home location
unsafe fn install_trans(recovering: bool) {
    for tail in 0..LOG.lh.n as usize {
        let lbuf = bread(LOG.dev, LOG.start + tail as u32 + 1); // read log block
        let dbuf = bread(LOG.dev, LOG.lh.block[tail]); // read dst
        dbuf.data[..].clone_from_slice(&lbuf.data[..]);
        bwrite(dbuf); // write dst to disk
        if !recovering {
            bunpin(dbuf);
        }
        brelse(lbuf);
        brelse(dbuf);
    }
}

// Write in-memory log header to disk.
// This is the true point at which the
// current transaction commits.
unsafe fn write_head() {
    let buf = bread(LOG.dev, LOG.start);
    let (_head, body, _tail) = buf.data[0..mem::size_of::<LogHeader>()].align_to_mut::<LogHeader>();
    let mut hb = &mut body[0];
    hb.n = LOG.lh.n;
    for i in 0..LOG.lh.n as usize {
        hb.block[i] = LOG.lh.block[i];
    }
    bwrite(buf);
    brelse(buf);
}

// Caller has modified b->data and is done with the buffer.
// Record the block number and pin in the cache by increasing refcnt.
// commit()/write_log() will do the disk write.
//
// log_write() replaces bwrite(); a typical use is:
//   bp = bread(...)
//   modify bp->data[]
//   log_write(bp)
//   brelse(bp)
pub fn log_write(b: &mut Buf) {
    unsafe {
        LOG.lock.acquire();
        if LOG.lh.n as usize >= LOGSIZE || LOG.lh.n >= LOG.size - 1 {
            panic!("too big a transaction");
        }

        if LOG.outstanding < 1 {
            panic!("log_write outside of trans");
        }

        let mut idx = 0;
        for i in 0..LOG.lh.n as usize {
            if LOG.lh.block[i] == b.blockno {
                idx = i;
                break;
            }
        }

        LOG.lh.block[idx] = b.blockno;
        if idx == LOG.lh.n as usize {
            bpin(b);
            LOG.lh.n += 1;
        }

        LOG.lock.release();
    }
}
