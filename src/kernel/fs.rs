// On-disk file system format.
// Both the kernel and user programs use this header file.


pub const ROOTINO: usize = 1;   // root i-number
// pub const BSIZE: usize = 1024;  // block size
// TODO: due to the kernel stack issue(see entry.S), the block size cannot set too large, otherwise booting may failed
pub const BSIZE: usize = 128;  // block size