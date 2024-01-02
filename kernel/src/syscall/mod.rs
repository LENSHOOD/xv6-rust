mod sysfile;
mod syscall;

// System call numbers
pub const SYS_fork: u8 = 1;
pub const SYS_exit: u8 = 2;
pub const SYS_wait: u8 = 3;
pub const SYS_pipe: u8 = 4;
pub const SYS_read: u8 = 5;
pub const SYS_kill: u8 = 6;
pub const SYS_exec: u8 = 7;
pub const SYS_fstat: u8 = 8;
pub const SYS_chdir: u8 = 9;
pub const SYS_dup: u8 =  10;
pub const SYS_getpid: u8 = 11;
pub const SYS_sbrk: u8 = 12;
pub const SYS_sleep: u8 = 13;
pub const SYS_uptime: u8 = 14;
pub const SYS_open: u8 = 15;
pub const SYS_write: u8 = 16;
pub const SYS_mknod: u8 = 17;
pub const SYS_unlink: u8 = 18;
pub const SYS_link: u8 = 19;
pub const SYS_mkdir: u8 = 20;
pub const SYS_close: u8 = 21;

#[macro_export]
macro_rules! NELEM {
    ( $x:expr ) => {
        x.len()
    };
}