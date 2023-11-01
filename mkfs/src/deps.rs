use std::mem;


/*
    Followings are some constants and structs copied from kernel package.
    Due to dependency tree reasons, directly ref dependency from kernel package are painful, so I just copied them all to here.
 */
pub const MAXOPBLOCKS: u32 = 10;  // max # of blocks any FS op writes
pub const LOGSIZE: u32 = MAXOPBLOCKS*3;  // max data blocks in on-disk log

pub const FSSIZE: u32 = 2000;  // size of file system in blocks

pub const BSIZE: usize = 1024;  // block size

pub const IPB: u32 = (BSIZE / mem::size_of::<DINode>()) as u32;

pub const NDIRECT: usize = 12;

pub const ROOTINO: u32 = 1;

pub const NINDIRECT: usize = BSIZE / mem::size_of::<u32>();
pub const MAXFILE: usize = NDIRECT + NINDIRECT;

#[derive(Copy, Clone)]
pub enum FileType {
    NO_TYPE,
    T_DIR, // Directory
    T_FILE, // File
    T_DEVICE, // Device
}

#[repr(C)]
pub struct DINode {
    pub(crate) file_type: FileType, // File type
    pub(crate) major: i16, // Major device number (T_DEVICE only)
    pub(crate) minor: i16, // Minor device number (T_DEVICE only)
    pub(crate) nlink: i16, // Number of links to inode in file system
    pub(crate) size: u32, // Size of file (bytes)
    pub(crate) addrs: [u32; NDIRECT + 1], // Data block addresses
}

pub const FSMAGIC: u32 = 0x10203040;
#[repr(C)]
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

pub const DIRSIZ: usize = 14;

#[repr(C)]
pub struct Dirent {
    pub(crate) inum: u16,
    pub(crate) name: [u8; DIRSIZ],
}

#[macro_export]
macro_rules! IBLOCK {
    ( $i:expr, $sb:expr ) => {
        $i / IPB + $sb.inodestart
    };
}