use crate::pipe::Pipe;
use crate::sleeplock::Sleeplock;

enum FileType { FD_NONE, FD_PIPE, FD_INODE, FD_DEVICE }

#[derive(Default, Copy, Clone)]
pub struct File {
    file_type: FileType,
    ref_cnt: usize, // reference count
    readable: bool,
    writeable: bool,
    pipe: *Pipe,
    // TODO: inode
    // struct inode *ip;  // FD_INODE and FD_DEVICE
    // uint off;          // FD_INODE
    // short major;       // FD_DEVICE
}