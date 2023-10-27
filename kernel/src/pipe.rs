use crate::spinlock::Spinlock;

const PIPESIZE: usize = 512;

pub struct Pipe {
    lock: Spinlock,
    data: [u8; PIPESIZE],
    nread: u32, // number of bytes read
    nwrite: u32, // number of bytes written
    readopen: bool, // read fd is still open
    writeopen: bool, // write fd is still open
}