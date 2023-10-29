use std::mem;


/*
    Followings are some constants and structs copied from kernel package.
    Due to dependency tree reasons, directly ref dependency from kernel package are painful, so I just copied them all to here.
 */
pub const MAXOPBLOCKS: usize = 10;  // max # of blocks any FS op writes
pub const LOGSIZE: usize = MAXOPBLOCKS*3;  // max data blocks in on-disk log

pub const FSSIZE: usize = 2000;  // size of file system in blocks

pub const BSIZE: usize = 1024;  // block size

pub const IPB: usize = BSIZE / mem::size_of::<DINode>();

pub const NDIRECT: usize = 12;

pub const ROOTINO: usize = 1;

#[derive(Copy, Clone)]
pub enum FileType {
    NO_TYPE,
    T_DIR, // Directory
    T_FILE, // File
    T_DEVICE, // Device
}

pub struct DINode {
    file_type: FileType, // File type
    major: i16, // Major device number (T_DEVICE only)
    minor: i16, // Minor device number (T_DEVICE only)
    nlink: i16, // Number of links to inode in file system
    pub(crate) size: u32, // Size of file (bytes)
    addrs: [u32; NDIRECT + 1], // Data block addresses
}

pub const FSMAGIC:usize = 0x10203040;
pub struct SuperBlock {
    pub(crate) magic: u32, // Must be FSMAGIC
    pub(crate) size: u32, // Size of file system image (blocks)
    pub(crate) nblocks: u32, // Number of data blocks
    pub(crate) ninodes: u32, // Number of inodes.
    pub(crate) nlog: u32, // Number of log blocks
    pub(crate) logstart: u32, // Block number of first log block
    pub(crate) inodestart: u32, // Block number of first inode block
    pub(crate) bmapstart: u32, // Block number of first free map block
}

const DIRSIZ: usize = 14;

pub struct Dirent {
    pub(crate) inum: u16,
    name: [u8; DIRSIZ],
}