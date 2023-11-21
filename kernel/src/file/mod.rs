use crate::fs::NDIRECT;
use crate::param::NDEV;
use crate::pipe::Pipe;
use crate::sleeplock::Sleeplock;
use crate::stat::FileType;

pub mod file;

#[derive(Copy, Clone)]
enum FDType { FD_NONE, FD_PIPE, FD_INODE, FD_DEVICE }
#[derive(Copy, Clone)]
pub struct File<'a> {
    file_type: FDType,
    ref_cnt: i32, // reference count
    readable: bool,
    writable: bool,
    pipe: Option<&'a Pipe>, // FD_PIPE
    ip: Option<&'a INode>, // FD_INODE and FD_DEVICE
    off: u32, // FD_INODE
    major: i16, // FD_DEVICE
}

impl<'a> File<'a> {
    pub const fn create() -> Self {
        Self {
            file_type: FDType::FD_NONE,
            ref_cnt: 0,
            readable: false,
            writable: false,
            pipe: None,
            ip: None,
            off: 0,
            major: 0,
        }
    }
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
    pub(crate) ref_cnt: i32, // Reference count
    lock: Sleeplock, // protects everything below here
    valid: bool, // inode has been read from disk?

    pub(crate) file_type: FileType, // copy of disk inode
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

pub static mut DEVSW: [Option<&dyn Devsw>; NDEV] = [None; NDEV];

// map major device number to device functions.
pub trait Devsw {
    fn read(self: &Self, user_addr: usize, addr: usize, sz: usize);
    fn write(self: &Self, user_addr: usize, addr: usize, sz: usize);
}

pub const CONSOLE: usize = 1;
