#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::CONSOLE;
use kernel::file::fcntl::O_RDWR;
use ulib::printf;
use ulib::stubs::{dup, exec, exit, fork, mknod, open, wait};

#[start]
fn main(_argc: isize, argv: *const *const u8) -> isize {
    unsafe {
        if open("console" as *const str as *const u8, O_RDWR) < 0 {
            mknod("console" as *const str as *const u8, CONSOLE as u16, 0);
            open("console" as *const str as *const u8, O_RDWR);
        }
        dup(0);  // stdout
        dup(0);  // stderr

        let mut pid = 0;
        let mut wpid = 0;
        loop {
            printf!("init: starting sh\n");
            pid = fork();
            if pid < 0 {
                printf!("init: fork failed\n");
                exit(1);
            }
            if pid == 0 {
                exec("sh" as *const str as *const u8, argv);
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
