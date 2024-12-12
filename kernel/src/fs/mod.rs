// On-disk file system format.
// Both the kernel and user programs use this header file.

use core::mem;

use crate::stat::FileType;

pub(crate) mod fs;

pub const ROOTINO: u32 = 1; // root i-number
pub const BSIZE: usize = 4096; // block size

// Disk layout:
// [ boot block | super block | log | inode blocks |
//                                          free bit map | data blocks]
//
// mkfs computes the super block and builds an initial file system. The
// super block describes the disk layout:
pub struct SuperBlock {
    pub magic: u32,               // Must be FSMAGIC
    pub size: u32,                // Size of file system image (blocks)
    pub nblocks: u32,             // Number of data blocks
    pub ninodes: u32,             // Number of inodes.
    pub nlog: u32,     // Number of log blocks
    pub logstart: u32, // Block number of first log block
    pub inodestart: u32,          // Block number of first inode block
    pub bmapstart: u32,           // Block number of first free map block
}

pub const FSMAGIC: u32 = 0x10203040;
pub const NDIRECT: usize = 12;
pub const NINDIRECT: usize = BSIZE / mem::size_of::<u32>(); // BSIZE / sizeof(uint)
pub const MAXFILE: usize = NDIRECT + NINDIRECT;

// On-disk inode structure
#[repr(C)]
#[derive(Copy, Clone)]
pub struct DINode {
    pub file_type: FileType,       // File type
    pub major: i16,                // Major device number (T_DEVICE only)
    pub minor: i16,                // Minor device number (T_DEVICE only)
    pub nlink: i16,                // Number of links to inode in file system
    pub size: u32,                 // Size of file (bytes)
    pub addrs: [u32; NDIRECT + 1], // Data block addresses
}

// Inodes per block.
pub const IPB: u32 = (BSIZE / mem::size_of::<DINode>()) as u32;

// Block containing inode i
#[macro_export]
macro_rules! IBLOCK {
    ( $i:expr, $sb:expr ) => {
        $i / IPB + $sb.inodestart
    };
}

// Bitmap bits per block
const BPB: u32 = (BSIZE * 8) as u32;

// Block of free map containing bit for block b
#[macro_export]
macro_rules! BBLOCK {
    ( $b:expr, $sb:expr ) => {
        $b / BPB + $sb.bmapstart
    };
}

// Directory is a file containing a sequence of dirent structures.
pub const DIRSIZ: usize = 14;

pub struct Dirent {
    pub inum: u16,
    pub name: [u8; DIRSIZ],
}
