use crate::fs::NDIRECT;
use crate::param::NDEV;
use crate::pipe::Pipe;
use crate::sleeplock::Sleeplock;
use crate::stat::FileType;

pub mod file;
pub mod fcntl;

#[derive(Copy, Clone, PartialEq)]
pub(crate) enum FDType { FD_NONE, FD_PIPE, FD_INODE, FD_DEVICE }
#[derive(Copy, Clone)]
pub struct File {
    pub(crate) file_type: FDType,
    ref_cnt: i32, // reference count
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pipe: Option<*mut Pipe>, // FD_PIPE
    pub(crate) ip: Option<*mut INode>, // FD_INODE and FD_DEVICE
    pub(crate) off: u32, // FD_INODE
    pub(crate) major: i16, // FD_DEVICE
}

impl File {
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
    pub(crate) dev: u32, // Device number
    pub(crate) inum: u32, // Inode number
    pub(crate) ref_cnt: i32, // Reference count
    pub(crate) lock: Sleeplock, // protects everything below here
    pub(crate) valid: bool, // inode has been read from disk?

    pub(crate) file_type: FileType, // copy of disk inode
    pub(crate) major: i16,
    pub(crate) minor: i16,
    pub(crate) nlink: i16,
    pub(crate) size: u32,
    pub(crate) addrs: [u32; NDIRECT + 1]
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

pub static mut DEVSW: [Option<*mut dyn Devsw>; NDEV] = [None; NDEV];

// map major device number to device functions.
pub trait Devsw {
    fn read(self: &mut Self, is_user_dst: bool, dst: usize, sz: usize) -> i32;
    fn write(self: &mut Self, is_user_src: bool, src: usize, sz: usize) -> i32;
}

pub const CONSOLE: usize = 1;
