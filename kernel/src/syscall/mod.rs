mod sysfile;
pub(crate) mod syscall;
mod sysproc;

// System call numbers
pub const SYS_fork: usize = 1;
pub const SYS_exit: usize = 2;
pub const SYS_wait: usize = 3;
pub const SYS_pipe: usize = 4;
pub const SYS_read: usize = 5;
pub const SYS_kill: usize = 6;
pub const SYS_exec: usize = 7;
pub const SYS_fstat: usize = 8;
pub const SYS_chdir: usize = 9;
pub const SYS_dup: usize =  10;
pub const SYS_getpid: usize = 11;
pub const SYS_sbrk: usize = 12;
pub const SYS_sleep: usize = 13;
pub const SYS_uptime: usize = 14;
pub const SYS_open: usize = 15;
pub const SYS_write: usize = 16;
pub const SYS_mknod: usize = 17;
pub const SYS_unlink: usize = 18;
pub const SYS_link: usize = 19;
pub const SYS_mkdir: usize = 20;
pub const SYS_close: usize = 21;