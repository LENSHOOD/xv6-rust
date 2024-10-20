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
use crate::fs::{
    DINode, Dirent, SuperBlock, BPB, BSIZE, DIRSIZ, FSMAGIC, IPB, MAXFILE, NDIRECT, NINDIRECT,
    ROOTINO,
};
use crate::log::{initlog, log_write};
use crate::param::{NINODE, ROOTDEV};
use crate::proc::{either_copyin, either_copyout, myproc};
use crate::spinlock::Spinlock;
use crate::stat::FileType;
use crate::stat::FileType::{NO_TYPE, T_DIR};
use crate::string::memset;
use crate::{printf, BBLOCK, IBLOCK};

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
    fn readsb(self: &mut Self, dev: u32) {
        let bp = bread(dev, 1);

        let sz = size_of_val(self);
        let raw =
            unsafe { core::slice::from_raw_parts_mut(self as *mut SuperBlock as *mut u8, sz) };
        raw.clone_from_slice(&bp.data[..sz]);
        brelse(bp);
    }
}

impl INode {
    // Increment reference count for ip.
    // Returns ip to enable ip = idup(ip1) idiom.
    pub(crate) fn idup(self: &mut Self) -> &mut Self {
        unsafe {
            ITABLE.lock.acquire();
            self.ref_cnt += 1;
            ITABLE.lock.release();
        }

        self
    }

    // Lock the given inode.
    // Reads the inode from disk if necessary.
    pub fn ilock(self: &mut Self) {
        if self.ref_cnt < 1 {
            panic!("ilock");
        }

        self.lock.acquire_sleep();

        if !self.valid {
            let bp = bread(self.dev, unsafe { IBLOCK!(self.inum, SB) });
            let ino_sz = mem::size_of::<DINode>();
            let offset = ino_sz * (self.inum % IPB) as usize;
            let (_head, body, _tail) =
                unsafe { bp.data[offset..offset + ino_sz].align_to::<DINode>() };
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
    pub(crate) fn iunlock(self: &mut Self) {
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
    pub(crate) fn iput(self: &mut Self) {
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
    pub fn iunlockput(self: &mut Self) {
        self.iunlock();
        self.iput();
    }

    // Truncate inode (discard contents).
    // Caller must hold ip->lock.
    pub(crate) fn itrunc(self: &mut Self) {
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
    pub(crate) fn iupdate(self: &mut Self) {
        let bp = bread(self.dev, unsafe { IBLOCK!(self.inum, SB) });
        let ino_sz = mem::size_of::<DINode>();
        let offset = ino_sz * (self.inum % IPB) as usize;
        let (_head, body, _tail) =
            unsafe { bp.data[offset..offset + ino_sz].align_to_mut::<DINode>() };
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
            let a: &mut [u32; NINDIRECT] = unsafe { mem::transmute(&mut (bp.data)) };
            addr = a[bn];
            if addr == 0 {
                addr = balloc(self.dev);
                if addr != 0 {
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
    pub(crate) fn readi<T>(
        self: &mut Self,
        is_user_dst: bool,
        dst: *mut T,
        off: u32,
        n: usize,
    ) -> usize {
        let mut n = n as u32;
        if off > self.size || off + n < off {
            return 0;
        }

        if off + n > self.size {
            n = self.size - off;
        }

        let mut tot = 0;
        let mut off = off;
        let mut dst = dst;
        loop {
            if tot >= n {
                break;
            }
            let addr = self.bmap(off / BSIZE as u32);
            if addr == 0 {
                break;
            }

            let bp = bread(self.dev, addr);
            let m = min(n - tot, (BSIZE - off as usize % BSIZE) as u32);
            if either_copyout(
                is_user_dst,
                dst as *mut u8,
                &bp.data[off as usize % BSIZE] as *const u8,
                m as usize,
            ) == -1
            {
                brelse(bp);
                tot = 0;
                break;
            }
            brelse(bp);

            tot += m;
            off += m;
            dst = unsafe { dst.add(m as usize) };
        }

        return tot as usize;
    }

    // Write data to inode.
    // Caller must hold ip->lock.
    // If user_src==1, then src is a user virtual address;
    // otherwise, src is a kernel address.
    // Returns the number of bytes successfully written.
    // If the return value is less than the requested n,
    // there was an error of some kind.
    pub(crate) fn writei<T>(
        self: &mut Self,
        is_user_src: bool,
        src: *mut T,
        off: u32,
        n: usize,
    ) -> isize {
        let n = n as u32;
        if off > self.size || (off + n) < off {
            return -1;
        }

        if off + n > (MAXFILE * BSIZE) as u32 {
            return -1;
        }

        let mut tot = 0;
        let mut off = off;
        let mut src = src;
        loop {
            if tot >= n {
                break;
            }

            let addr = self.bmap(off / BSIZE as u32);
            if addr == 0 {
                break;
            }

            let bp = bread(self.dev, addr);
            let m = min(n - tot, (BSIZE - off as usize % BSIZE) as u32);
            if either_copyin(
                &mut bp.data[off as usize % BSIZE] as *mut u8,
                is_user_src,
                src as *const u8,
                m as usize,
            ) == -1
            {
                brelse(bp);
                break;
            }
            log_write(bp);
            brelse(bp);

            tot += m;
            off += m;
            src = unsafe { src.add(m as usize) };
        }

        if off > self.size {
            self.size = off;
        }

        // write the i-node back to disk even if the size didn't change
        // because the loop above might have called bmap() and added a new
        // block to ip->addrs[].
        self.iupdate();

        return tot as isize;
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

pub(crate) fn namei<'a>(path: &[u8]) -> Option<&'a mut INode> {
    let (inode_op, _subpath) = namex(path, false);
    return inode_op;
}

pub(crate) fn nameiparent<'a>(path: &[u8]) -> (Option<&'a mut INode>, &[u8]) {
    let (inode_op, subpath) = namex(path, true);
    return (inode_op, &path[subpath.name.0..subpath.name.1]);
}

// Look up and return the inode for a path name.
// If parent != 0, return the inode for the parent and copy the final
// path element into name, which must have room for DIRSIZ bytes.
// Must be called inside a transaction since it calls iput().
fn namex<'a>(path: &[u8], nameiparent: bool) -> (Option<&'a mut INode>, SubPath) {
    let mut ip = if path.len() >= 1 && path[0] == b'/' {
        iget(ROOTDEV, ROOTINO)
    } else {
        let inode = myproc().cwd.unwrap();
        unsafe { inode.as_mut().unwrap().idup() }
    };

    let mut sb = SubPath {
        raw: path,
        subpath: Some(0),
        name: (0, 0),
    };

    loop {
        sb = skipelem(sb);
        if sb.subpath.is_none() {
            break;
        }

        ip.ilock();
        if ip.file_type != T_DIR {
            ip.iunlockput();
            return (None, sb);
        }

        if nameiparent && sb.raw[sb.subpath.unwrap()] == b'\0' {
            // Stop one level early.
            ip.iunlock();
            return (Some(ip), sb);
        }

        match dirlookup(ip, &sb.raw[sb.name.0..sb.name.1 + 1], &mut 0) {
            next => {
                if next.is_none() {
                    ip.iunlockput();
                    return (None, sb);
                }

                ip.iunlockput();
                ip = next.unwrap();
            }
        }
    }

    if nameiparent {
        ip.iput();
        return (None, sb);
    }

    return (Some(ip), sb);
}

// Allocate an inode on device dev.
// Mark it as allocated by  giving it type type.
// Returns an unlocked but allocated and referenced inode,
// or NULL if there is no free inode.
pub(crate) fn ialloc<'a>(dev: u32, file_type: FileType) -> Option<&'a mut INode> {
    for inum in 1..unsafe { SB.ninodes } {
        let bp = bread(dev, unsafe { IBLOCK!(inum, SB) });
        let (_head, body, _tail) = unsafe {
            let ino_sz = mem::size_of::<DINode>();
            bp.data[ino_sz * (inum % IPB) as usize..ino_sz * ((inum + 1) % IPB) as usize]
                .align_to_mut::<DINode>()
        };
        let dip = &mut body[0];
        if dip.file_type == NO_TYPE {
            memset(dip as *mut DINode as *mut u8, 0, mem::size_of::<DINode>());
            dip.file_type = file_type;
            log_write(bp);
            brelse(bp);
            return Some(iget(dev, inum));
        }
        brelse(bp);
    }
    printf!("ialloc: no inodes\n");
    return None;
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
        ip.valid = false;

        ITABLE.lock.release();

        return ip;
    }
}

struct SubPath<'a> {
    raw: &'a [u8],
    subpath: Option<usize>,
    name: (usize, usize),
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
fn skipelem(sb: SubPath) -> SubPath {
    if sb.subpath.is_none() {
        return sb;
    }

    let mut subpath_idx = sb.subpath.unwrap();
    while subpath_idx < sb.raw.len() && sb.raw[subpath_idx] == b'/' {
        subpath_idx += 1;
    }

    if subpath_idx == sb.raw.len() || sb.raw[subpath_idx] == b'\0' {
        return SubPath {
            raw: sb.raw,
            subpath: None,
            name: (0, 0),
        };
    }

    let name_start = subpath_idx;
    while subpath_idx < sb.raw.len() && sb.raw[subpath_idx] != b'/' && sb.raw[subpath_idx] != b'\0'
    {
        subpath_idx += 1;
    }
    let mut name_end = subpath_idx - name_start;
    if name_end > DIRSIZ {
        name_end = DIRSIZ;
    }

    while subpath_idx < sb.raw.len() && sb.raw[subpath_idx] == b'/' {
        subpath_idx += 1;
    }

    SubPath {
        raw: sb.raw,
        subpath: Some(subpath_idx),
        name: (name_start, name_end),
    }
}

// Look for a directory entry in a directory.
// If found, set *poff to byte offset of entry.
pub(crate) fn dirlookup<'a>(dp: &mut INode, name: &[u8], poff: &mut u32) -> Option<&'a mut INode> {
    if dp.file_type != T_DIR {
        panic!("dirlookup not DIR");
    }

    let mut de = Dirent {
        inum: 0,
        name: [0; DIRSIZ],
    };

    let mut dir_name = [0u8; DIRSIZ];
    let len = min(name.len(), dir_name.len());
    dir_name[..len].clone_from_slice(name);

    let sz = mem::size_of::<Dirent>();
    for off in (0..dp.size).step_by(sz) {
        // clear name buffer
        de.name = [0; DIRSIZ];

        if dp.readi(false, &mut de as *mut Dirent, off, sz) != sz {
            panic!("dirlookup read");
        }

        if de.inum == 0 {
            continue;
        }

        if dir_name.eq(&de.name) {
            // entry matches path element
            if *poff != 0 {
                *poff = off;
            }
            return Some(iget(dp.dev, de.inum as u32));
        }
    }

    None
}

// Write a new directory entry (name, inum) into the directory dp.
// Returns 0 on success, -1 on failure (e.g. out of disk blocks).
pub(crate) fn dirlink(dp: &mut INode, name: &[u8], inum: u16) -> Option<()> {
    // Check that name is not present.
    let ip = dirlookup(dp, name, &mut 0);
    if ip.is_some() {
        ip?.iput();
        return None;
    }

    // Look for an empty dirent.
    let de = &mut Dirent {
        inum: 0,
        name: [0; DIRSIZ],
    };
    let sz = mem::size_of::<Dirent>();
    let mut off = 0;
    loop {
        if off >= dp.size {
            break;
        }

        if dp.readi(false, de as *mut Dirent, off, sz) == 0 {
            panic!("dirlink read");
        }

        if de.inum == 0 {
            break;
        }

        off += sz as u32;
    }

    de.name[..name.len()].clone_from_slice(name);
    de.inum = inum;

    if dp.writei(false, de as *mut Dirent, off, sz) == 0 {
        return None;
    }

    return Some(());
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
                bp.data[bi as usize / 8] |= m; // Mark block in use.
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
