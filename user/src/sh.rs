#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::fcntl::O_RDWR;
use kernel::string::{memset, strlen};
use ulib::{fprintf, gets};
use ulib::stubs::{chdir, close, dup, exec, exit, fork, mknod, open, read, wait, write};

// Parsed command representation
const EXEC: i32 = 1;
const REDIR: i32 = 2;
const PIPE: i32 = 3;
const LIST: i32 = 4;
const BACK: i32 = 5;
const MAXARGS: usize = 10;
struct Cmd {
    cmd_type: i32
}

struct ExecCmd {
    cmd_type: i32,
    argv: [u8; MAXARGS],
    eargv: [u8; MAXARGS],
}

struct RedirCmd<'a> {
    cmd_type: i32,
    cmd: &'a Cmd,
    file: *const u8,
    efile: *const u8,
    mode: i32,
    fd: i32
}

struct PipeCmd<'a> {
    cmd_type: i32,
    left: &'a Cmd,
    right: &'a Cmd,
}

struct ListCmd<'a> {
    cmd_type: i32,
    left: &'a Cmd,
    right: &'a Cmd,
}

struct BackCmd<'a> {
    cmd_type: i32,
    cmd: &'a Cmd,
}

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
            if unsafe { chdir(buf+3) } < 0 {
                fprintf!(2, "cannot cd {}\n", buf[3..]);
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

fn fork1() -> i32 {
    let pid = unsafe { fork() };
    if pid == -1 {
        panic!("fork");
    }
    return pid;
}