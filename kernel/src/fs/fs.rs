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

use core::cmp::min;
use core::mem;
use core::mem::size_of_val;
use crate::bio::{bread, brelse};
use crate::file::INode;
use crate::fs::{BPB, BSIZE, DINode, Dirent, DIRSIZ, FSMAGIC, IPB, NDIRECT, NINDIRECT, ROOTINO, SuperBlock};
use crate::{BBLOCK, IBLOCK, printf};
use crate::log::{initlog, log_write};
use crate::param::{NINODE, ROOTDEV};
use crate::proc::{either_copyout, myproc};
use crate::spinlock::Spinlock;
use crate::stat::FileType::{NO_TYPE, T_DIR};
use crate::string::memset;

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
    fn ilock(self: &mut Self) {
        if self.ref_cnt < 1 {
            panic!("ilock");
        }

        self.lock.acquire_sleep();

        if !self.valid {
            let bp = bread(self.dev, unsafe { IBLOCK!(self.inum, SB) });
            let ino_sz = mem::size_of::<DINode>();
            let offset = ino_sz * (self.inum % IPB) as usize;
            let (_head, body, _tail) = unsafe {
                bp.data[offset..offset + ino_sz].align_to::<DINode>()
            };
            let dip = &body[0];
            self.file_type = dip.file_type;
            self.major = dip.major;
            self.minor = dip.minor;
            self.nlink = dip.nlink;
            self.size = dip.size;
            self.addrs.clone_from_slice(&dip.addrs);

            brelse(bp);
            self.valid = true;

            if self.file_type == NO_TYPE {
                panic!("ilock: no type");
            }
        }
    }

    // Unlock the given inode.
    fn iunlock(self: &mut Self) {
        if !self.lock.holding_sleep() || self.ref_cnt < 1 {
            panic!("iunlock");
        }

        self.lock.release_sleep();
    }

    // Drop a reference to an in-memory inode.
    // If that was the last reference, the inode table entry can
    // be recycled.
    // If that was the last reference and the inode has no links
    // to it, free the inode (and its content) on disk.
    // All calls to iput() must be inside a transaction in
    // case it has to free the inode.
    fn iput(self: &mut Self) {
        unsafe {
            ITABLE.lock.acquire();

            if self.ref_cnt == 1 && self.valid && self.nlink == 0 {
                // inode has no links and no other references: truncate and free.

                // ip->ref == 1 means no other process can have ip locked,
                // so this acquiresleep() won't block (or deadlock).
                self.lock.acquire_sleep();

                ITABLE.lock.release();

                self.itrunc();
                self.file_type = NO_TYPE;
                self.iupdate();
                self.valid = false;

                self.lock.release_sleep();

                ITABLE.lock.acquire();
            }

            self.ref_cnt -= 1;
            ITABLE.lock.release();
        }
    }
    // Common idiom: unlock, then put.
    fn iunlockput(self: &mut Self) {
        self.iunlock();
        self.iput();
    }

    // Truncate inode (discard contents).
    // Caller must hold ip->lock.
    fn itrunc(self: &mut Self) {
        for i in 0..NDIRECT {
            if self.addrs[i] != 0 {
                bfree(self.dev, self.addrs[i]);
                self.addrs[i] = 0;
            }
        }

        if self.addrs[NDIRECT] != 0 {
            let bp = bread(self.dev, self.addrs[NDIRECT]);
            let a: [u32; NINDIRECT] = unsafe { mem::transmute(bp.data) };
            for i in 0..NINDIRECT {
                if a[i] != 0 {
                    bfree(self.dev, a[i])
                }
            }
            brelse(bp);
            bfree(self.dev, self.addrs[NDIRECT]);
            self.addrs[NDIRECT] = 0;
        }

        self.size = 0;
        self.iupdate();
    }

    // Copy a modified in-memory inode to disk.
    // Must be called after every change to an ip->xxx field
    // that lives on disk.
    // Caller must hold ip->lock.
    fn iupdate(self: &mut Self) {
        let bp = bread(self.dev, unsafe { IBLOCK!(self.inum, SB) });
        let ino_sz = mem::size_of::<DINode>();
        let offset = ino_sz * (self.inum % IPB) as usize;
        let (_head, body, _tail) = unsafe {
            bp.data[offset..offset + ino_sz].align_to_mut::<DINode>()
        };
        let dip = &mut body[0];
        dip.file_type = self.file_type;
        dip.major = self.major;
        dip.minor = self.minor;
        dip.nlink = self.nlink;
        dip.size = self.size;
        dip.addrs.clone_from_slice(&self.addrs);
        log_write(bp);
        brelse(bp);
    }

    // Inode content
    //
    // The content (data) associated with each inode is stored
    // in blocks on the disk. The first NDIRECT block numbers
    // are listed in ip->addrs[].  The next NINDIRECT blocks are
    // listed in block ip->addrs[NDIRECT].

    // Return the disk block address of the nth block in inode ip.
    // If there is no such block, bmap allocates one.
    // returns 0 if out of disk space.
    fn bmap(self: &mut Self, bn: u32) -> u32 {
        let mut bn = bn as usize;
        if bn < NDIRECT {
            let mut addr = self.addrs[bn];
            if addr == 0 {
                addr = balloc(self.dev);
                if addr == 0 {
                    return 0;
                }
                self.addrs[bn] = addr;
            }
            return addr;
        }
        bn -= NDIRECT;

        if bn < NINDIRECT {
            // Load indirect block, allocating if necessary.
            let mut addr = self.addrs[NDIRECT];
            if addr == 0 {
                addr = balloc(self.dev);
                if addr == 0 {
                    return 0;
                }
                self.addrs[NDIRECT] = addr;
            }
            let bp = bread(self.dev, addr);
            let mut a: [u32; NINDIRECT] = unsafe { mem::transmute(bp.data) };
            addr = a[bn];
            if addr == 0 {
                addr = balloc(self.dev);
                if addr != 0{
                    a[bn] = addr;
                    log_write(bp);
                }
            }
            brelse(bp);
            return addr;
        }

        panic!("bmap: out of range");
    }

    // Read data from inode.
    // Caller must hold ip->lock.
    // If user_dst==1, then dst is a user virtual address;
    // otherwise, dst is a kernel address.
    fn readi<T>(self: &mut Self, is_user_dst: bool, dst: *mut T, off: u32, n: usize) -> usize {
        let mut n = n as u32;
        if off > self.size || off + n < off {
            return 0;
        }

        if off + n > self.size {
            n = self.size - off;
        }

        let mut tot = 0;
        loop {
            let addr = self.bmap(off/ BSIZE as u32);
            if addr == 0 {
                break;
            }

            let bp = bread(self.dev, addr);
            let m = min(n - tot, (BSIZE - off as usize % BSIZE) as u32);
            if either_copyout(is_user_dst, dst as *mut u8, &bp.data[off as usize % BSIZE] as *const u8, m as usize).is_err() {
                brelse(bp);
                tot = 0;
                break;
            }
            brelse(bp);
        }

        return tot as usize;
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

    while let Some(p) = skipelem(path) {
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
    unsafe {
        ITABLE.lock.acquire();

        // Is the inode already in the table?
        let mut empty: Option<&mut INode> = None;
        for ip in &mut ITABLE.inode {
            if ip.ref_cnt > 0 && ip.dev == dev && ip.inum == inum {
                ip.ref_cnt += 1;
                ITABLE.lock.release();
                return ip;
            }

            // Remember empty slot.
            if empty.is_none() && ip.ref_cnt == 0 {
               empty = Some(ip);
            }
        }

        // Recycle an inode entry.
        if empty.is_none() {
            panic!("iget: no inodes");
        }

        let ip = empty.unwrap();
        ip.dev = dev;
        ip.inum = inum;
        ip.ref_cnt = 1;
        ip.valid = true;

        ITABLE.lock.release();

        return ip;
    }
}

struct SubPath<'a> {
    subpath: Option<&'a str>,
    name: &'a str,
}

// Paths

// Copy the next path element from path into name.
// Return a pointer to the element following the copied one.
// The returned path has no leading slashes,
// so the caller can check *path=='\0' to see if the name is the last one.
// If no name to remove, return 0.
//
// Examples:
//   skipelem("a/bb/c", name) = "bb/c", setting name = "a"
//   skipelem("///a//bb", name) = "bb", setting name = "a"
//   skipelem("a", name) = "", setting name = "a"
//   skipelem("", name) = skipelem("////", name) = 0
//
fn skipelem(path: &str) -> Option<SubPath> {
    let path_slice = path.as_bytes();
    let mut i = 0;
    for c in path_slice {
        if *c != b'/' {
            break;
        }

        i += 1;
    }

    if i == path_slice.len() {
        return None;
    }

    let path_slice = &path_slice[i..];
    let s = path_slice;

    i = 0;
    for c in path_slice {
        if *c == b'/' {
            break;
        }

        i += 1;
    }

    let mut sub_path = SubPath {
        subpath: None,
        name: "",
    };

    if i > DIRSIZ {
        i = DIRSIZ;
    }
    sub_path.name = unsafe { core::str::from_utf8_unchecked(&s[..i]) };

    let path_slice = &path_slice[i..];
    i = 0;
    for c in path_slice {
        if *c != b'/' {
            break;
        }

        i += 1;
    }
    sub_path.subpath = Some(unsafe { core::str::from_utf8_unchecked(&path_slice[i..]) });

    return Some(sub_path);
}

// Look for a directory entry in a directory.
// If found, set *poff to byte offset of entry.
fn dirlookup<'a>(dp: &mut INode, name: &str, poff: &mut u32) -> Option<&'a mut INode> {
    if dp.file_type != T_DIR {
        panic!("dirlookup not DIR");
    }

    let mut de = Dirent {
        inum: 0,
        name: [0; DIRSIZ],
    };

    let sz = mem::size_of::<Dirent>();
    for off in (0..dp.size).step_by(sz) {

        if dp.readi(false, &mut de as *mut Dirent, off, sz) != sz {
            panic!("dirlookup read");
        }

        if de.inum == 0 {
            continue;
        }

        if name.as_bytes().eq(&de.name) {
            // entry matches path element
            if *poff != 0 {
                *poff = off;
            }
            return Some(iget(dp.dev, de.inum as u32));
        }
    }

    None
}

// Zero a block.
fn bzero(dev: u32, bno: u32) {
    let bp = bread(dev, bno);
    memset(&mut bp.data as *mut u8, 0, BSIZE);
    log_write(bp);
    brelse(bp);
}

// Blocks.

// Allocate a zeroed disk block.
// returns 0 if out of disk space.
fn balloc(dev: u32) -> u32 {
    let sz = unsafe { SB.size };
    for b in (0..sz).step_by(BPB as usize) {
        let bp = bread(dev, unsafe { BBLOCK!(b, SB) });
        let mut bi = 0;
        loop {
            if !(bi < BPB && b + bi < sz) {
                break;
            }

            let m = 1 << (bi % 8);
            if (bp.data[bi as usize / 8] & m) == 0 {
                bp.data[bi as usize / 8] |= m;  // Mark block in use.
                log_write(bp);
                brelse(bp);
                bzero(dev, b + bi);
                return b + bi;
            }
            bi += 1;
        }
        brelse(bp);
    }
    printf!("balloc: out of blocks\n");
    return 0;
}

// Free a disk block.
fn bfree(dev: u32, b: u32) {
    let bp = bread(dev, unsafe { BBLOCK!(b, SB) });
    let bi = b % BPB;
    let m = 1 << (bi % 8);
    if (bp.data[bi as usize / 8] & m) == 0 {
        panic!("freeing free block");
    }
    bp.data[bi as usize / 8] &= !m;
    log_write(bp);
    brelse(bp);
}