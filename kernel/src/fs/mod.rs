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
    magic: u32,               // Must be FSMAGIC
    size: u32,                // Size of file system image (blocks)
    nblocks: u32,             // Number of data blocks
    ninodes: u32,             // Number of inodes.
    pub(crate) nlog: u32,     // Number of log blocks
    pub(crate) logstart: u32, // Block number of first log block
    inodestart: u32,          // Block number of first inode block
    bmapstart: u32,           // Block number of first free map block
}

const FSMAGIC: u32 = 0x10203040;
pub const NDIRECT: usize = 12;
const NINDIRECT: usize = BSIZE / mem::size_of::<u32>(); // BSIZE / sizeof(uint)
const MAXFILE: usize = NDIRECT + NINDIRECT;

// On-disk inode structure
#[repr(C)]
struct DINode {
    pub(crate) file_type: FileType,       // File type
    pub(crate) major: i16,                // Major device number (T_DEVICE only)
    pub(crate) minor: i16,                // Minor device number (T_DEVICE only)
    pub(crate) nlink: i16,                // Number of links to inode in file system
    pub(crate) size: u32,                 // Size of file (bytes)
    pub(crate) addrs: [u32; NDIRECT + 1], // Data block addresses
}

// Inodes per block.
const IPB: u32 = (BSIZE / mem::size_of::<DINode>()) as u32;

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
const DIRSIZ: usize = 14;

struct Dirent {
    inum: u16,
    name: [u8; DIRSIZ],
}
