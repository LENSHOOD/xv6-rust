use crate::spinlock::Spinlock;

const PIPESIZE: usize = 512;

pub struct Pipe {
    lock: Spinlock,
    data: [u8; PIPESIZE],
    nread: usize,     // number of bytes read
    nwrite: usize,    // number of bytes written
    readopen: u8,   // read fd is still open
    writeopen: u8  // write fd is still open
}