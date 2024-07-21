#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::CONSOLE;
use kernel::file::fcntl::O_RDWR;
use kernel::string::{memset, strlen};
use ulib::printf;
use ulib::stubs::{close, dup, exec, exit, fork, mknod, open, wait, write};

#[start]
fn main(_argc: isize, _argv: *const *const u8) -> isize {
    // Ensure that three file descriptors are open.
    loop {
        let fd = unsafe { open("console\0".as_bytes().as_ptr(), O_RDWR) };
        if fd < 0 { 
            break;
        }
        
        if fd >= 3 {
            unsafe { close(fd); }
            break;
        }
    }

    // Read and run input commands.
    let mut buf_raw: [u8; 100] = [0; 100];
    let buf = buf_raw.as_mut_ptr();
    while getcmd(buf, buf.len()) >= 0 {
        if buf[0] == b'c' && buf[1] == b'd' && buf[2] == b' ' {
            // Chdir must be called by the parent, not the child.
            buf[strlen(buf)-1] = 0;  // chop \n
            if chdir(buf+3) < 0 {
                fprintf(2, "cannot cd %s\n", buf+3);
            }
            continue;
        }
        if fork1() == 0 {
            runcmd(parsecmd(buf));
        }
        unsafe { wait(0 as *const u8); }
    }

    unsafe { exit(0); }
}

fn getcmd(buf: *mut u8, nbuf: usize) -> i32 {
    unsafe { write(2, "$ \0".as_bytes().as_ptr(), 2); }
    memset(buf, 0, nbuf);
    gets(buf, nbuf);
    if buf[0] == 0 { // EOF
        return -1;
    }
    
    return 0;
}

