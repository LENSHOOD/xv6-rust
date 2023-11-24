use crate::buf::Buf;
use crate::fs::SuperBlock;

pub fn initlog(_dev: u32, _sb: &SuperBlock) {
    todo!()
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
pub fn log_write(_b: &Buf) {
    todo!()
}
