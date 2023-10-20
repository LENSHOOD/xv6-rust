// On-disk file system format.
// Both the kernel and user programs use this header file.


pub const ROOTINO: usize = 1;   // root i-number
pub const BSIZE: usize = 1024;  // block size