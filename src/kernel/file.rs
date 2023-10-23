use crate::fs::NDIRECT;
use crate::pipe::Pipe;
use crate::sleeplock::Sleeplock;
use crate::stat::FileType;

enum FDType { FD_NONE, FD_PIPE, FD_INODE, FD_DEVICE }
struct File<'a> {
    file_type: FDType,
    ref_cnt: i32, // reference count
    readable: bool,
    writable: bool,
    pipe: &'a Pipe, // FD_PIPE
    ip: &'a INode, // FD_INODE and FD_DEVICE
    off: u32, // FD_INODE
    major: i16, // FD_DEVICE
}

#[macro_export]
macro_rules! major {
    ( $dev:expr ) => {
        $dev >> 16 & 0xFFFF
    };
}

#[macro_export]
macro_rules! minor {
    ( $dev:expr ) => {
        $dev & 0xFFFF
    };
}

#[macro_export]
macro_rules! mkdev {
    ( $m:expr, $n:expr ) => {
        ($m << 16 | $n) as u32
    };
}

// in-memory copy of an inode
#[derive(Copy, Clone)]
pub struct INode {
    dev: u32, // Device number
    inum: u32, // Inode number
    ref_cnt: i32, // Reference count
    lock: Sleeplock, // protects everything below here
    valid: bool, // inode has been read from disk?

    file_type: FileType, // copy of disk inode
    major: i16,
    minor: i16,
    nlink: i16,
    size: u32,
    addrs: [u32; NDIRECT + 1]
}

impl INode {
    pub const fn create(lock_name: &'static str) -> Self {
        Self {
            dev: 0,
            inum: 0,
            ref_cnt: 0,
            lock: Sleeplock::init_lock(lock_name),
            valid: false,
            file_type: FileType::NO_TYPE,
            major: 0,
            minor: 0,
            nlink: 0,
            size: 0,
            addrs: [0; NDIRECT + 1],
        }
    }
}

// TODO
// map major device number to device functions.
// struct devsw {
//     int (*read)(int, uint64, int);
//     int (*write)(int, uint64, int);
// };
//
// extern struct devsw devsw[];

const CONSOLE: usize = 1;
