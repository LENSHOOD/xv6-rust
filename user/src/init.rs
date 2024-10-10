#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::fcntl::O_RDWR;
use kernel::file::CONSOLE;
use ulib::printf;
use ulib::stubs::{dup, exec, exit, fork, mknod, open, wait};

#[start]
fn main(_argc: isize, _argv: *const *const u8) -> isize {
    unsafe {
        // let mut console_slice: [u8; MAXPATH] = [b'\0'; MAXPATH];
        // console_slice.copy_from_slice("console".as_bytes());
        let console_slice = "console\0".as_bytes();
        if open(console_slice.as_ptr(), O_RDWR) < 0 {
            mknod(console_slice.as_ptr(), CONSOLE as u16, 0);
            open(console_slice.as_ptr(), O_RDWR);
        }
        dup(0); // stdout fd=1
        dup(0); // stderr fd=2

        let mut pid;
        let mut wpid;
        loop {
            printf!("init: starting sh\n");
            pid = fork();
            if pid < 0 {
                printf!("init: fork failed\n");
                exit(1);
            }
            if pid == 0 {
                let argv: *const *const u8 =
                    (&["sh\0".as_bytes().as_ptr(), "".as_bytes().as_ptr()]).as_ptr();
                exec("sh\0".as_bytes().as_ptr(), argv);
                printf!("init: exec sh failed\n");
                exit(1);
            }

            loop {
                // this call to wait() returns if the shell exits,
                // or if a parentless process exits.
                wpid = wait(0 as *const u8);
                if wpid == pid {
                    // the shell exited; restart it.
                    break;
                } else if wpid < 0 {
                    printf!("init: wait returned an error\n");
                    exit(1);
                } else {
                    // it was a parentless process; do nothing.
                }
            }
        }
    }
}
