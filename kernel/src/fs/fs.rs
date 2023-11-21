// Inodes.
//
// An inode describes a single unnamed file.
// The inode disk structure holds metadata: the file's type,
// its size, the number of links referring to it, and the
// list of blocks holding the file's content.
//
// The inodes are laid out sequentially on disk at block
// sb.inodestart. Each inode has a number, indicating its
// position on the disk.
//
// The kernel keeps a table of in-use inodes in memory
// to provide a place for synchronizing access
// to inodes used by multiple processes. The in-memory
// inodes include book-keeping information that is
// not stored on disk: ip->ref and ip->valid.
//
// An inode and its in-memory representation go through a
// sequence of states before they can be used by the
// rest of the file system code.
//
// * Allocation: an inode is allocated if its type (on disk)
//   is non-zero. ialloc() allocates, and iput() frees if
//   the reference and link counts have fallen to zero.
//
// * Referencing in table: an entry in the inode table
//   is free if ip->ref is zero. Otherwise ip->ref tracks
//   the number of in-memory pointers to the entry (open
//   files and current directories). iget() finds or
//   creates a table entry and increments its ref; iput()
//   decrements ref.
//
// * Valid: the information (type, size, &c) in an inode
//   table entry is only correct when ip->valid is 1.
//   ilock() reads the inode from
//   the disk and sets ip->valid, while iput() clears
//   ip->valid if ip->ref has fallen to zero.
//
// * Locked: file system code may only examine and modify
//   the information in an inode and its content if it
//   has first locked the inode.
//
// Thus a typical sequence is:
//   ip = iget(dev, inum)
//   ilock(ip)
//   ... examine and modify ip->xxx ...
//   iunlock(ip)
//   iput(ip)
//
// ilock() is separate from iget() so that system calls can
// get a long-term reference to an inode (as for an open file)
// and only lock it for short periods (e.g., in read()).
// The separation also helps avoid deadlock and races during
// pathname lookup. iget() increments ip->ref so that the inode
// stays in the table and pointers to it remain valid.
//
// Many internal file system functions expect the caller to
// have locked the inodes involved; this lets callers create
// multi-step atomic operations.
//
// The itable.lock spin-lock protects the allocation of itable
// entries. Since ip->ref indicates whether an entry is free,
// and ip->dev and ip->inum indicate which i-node an entry
// holds, one must hold itable.lock while using any of those fields.
//
// An ip->lock sleep-lock protects all ip-> fields other than ref,
// dev, and inum.  One must hold ip->lock in order to
// read or write that inode's ip->valid, ip->size, ip->type, &c.

use core::mem::size_of_val;
use crate::bio::{bread, brelse};
use crate::file::INode;
use crate::fs::{FSMAGIC, ROOTINO, SuperBlock};
use crate::log::initlog;
use crate::param::{NINODE, ROOTDEV};
use crate::proc::myproc;
use crate::spinlock::Spinlock;
use crate::stat::FileType::T_DIR;

struct ITable {
    lock: Spinlock,
    inode: [INode; NINODE],
}

static mut ITABLE: ITable = ITable {
    lock: Spinlock::init_lock("itable"),
    inode: [INode::create("inode"); NINODE],
};

pub fn iinit() {
    // empty due to ITABLE has already been initialized
}

static mut SB: SuperBlock = SuperBlock {
    magic: 0,
    size: 0,
    nblocks: 0,
    ninodes: 0,
    nlog: 0,
    logstart: 0,
    inodestart: 0,
    bmapstart: 0,
};

impl SuperBlock {
    fn readsb(self: &Self, dev: u32) {
        let bp = bread(dev, 1);

        let sz = size_of_val(self);
        let raw = unsafe { core::slice::from_raw_parts(self as *const SuperBlock as *const u8, sz) };
        bp.data[..sz].clone_from_slice(raw);
        brelse(bp);
    }
}

impl INode {
    // Increment reference count for ip.
    // Returns ip to enable ip = idup(ip1) idiom.
    fn idup(self: &mut Self) -> &mut Self {
        unsafe {
            ITABLE.lock.acquire();
            self.ref_cnt += 1;
            ITABLE.lock.release();
        }

        self
    }

    // Lock the given inode.
    // Reads the inode from disk if necessary.
    fn ilock(self: &Self) {
        todo!()
    }

    // Unlock the given inode.
    fn iunlock(self: &Self) {
        todo!()
    }

    // Drop a reference to an in-memory inode.
    // If that was the last reference, the inode table entry can
    // be recycled.
    // If that was the last reference and the inode has no links
    // to it, free the inode (and its content) on disk.
    // All calls to iput() must be inside a transaction in
    // case it has to free the inode.
    fn iput(self: &Self) {
        todo!()
    }
    // Common idiom: unlock, then put.
    fn iunlockput(self: &Self) {
        self.iunlock();
        self.iput();
    }
}

// Init fs
pub fn fsinit(dev: u32) {
    unsafe {
        SB.readsb(dev);
        if SB.magic != FSMAGIC {
            panic!("invalid file system");
        }
        initlog(dev, &SB);
    }
}

pub fn namei<'a>(path: &str) -> Option<&'a mut INode> {
    namex(path, false)
}

// Look up and return the inode for a path name.
// If parent != 0, return the inode for the parent and copy the final
// path element into name, which must have room for DIRSIZ bytes.
// Must be called inside a transaction since it calls iput().
fn namex<'a>(path: &str, nameiparent: bool) -> Option<&'a mut INode>{
    let mut ip = if path == "/" {
        iget(ROOTDEV, ROOTINO)
    } else {
        let mut inode = myproc().cwd?;
        unsafe { inode.as_mut()?.idup() }
    };

    while let p = skipelem(path) {
        ip.ilock();
        if ip.file_type != T_DIR {
            ip.iunlockput();
            return None;
        }

        if nameiparent && p.subpath.is_none() {
            // Stop one level early.
            ip.iunlock();
            return Some(ip);
        }

        if let next = dirlookup(ip, p.name, &mut 0) {
            if next.is_none() {
                ip.iunlockput();
                return None;
            }

            ip.iunlockput();
            ip = next.unwrap();
        }

        if nameiparent {
            ip.iput();
            return None;
        }
    }

    return Some(ip);
}

// Find the inode with number inum on device dev
// and return the in-memory copy. Does not lock
// the inode and does not read it from disk.
fn iget<'a>(dev: u32, inum: u32) -> &'a mut INode {
    todo!()
}

struct SubPath<'a> {
    subpath: Option<&'a str>,
    name: &'a str,
}
fn skipelem(path: &str) -> SubPath {
    todo!()
}

// Look for a directory entry in a directory.
// If found, set *poff to byte offset of entry.
fn dirlookup<'a>(dp: &INode, name: &str, poff: &mut u32) -> Option<&'a mut INode> {
    todo!()
}