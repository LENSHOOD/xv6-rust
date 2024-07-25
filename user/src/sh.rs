#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::fcntl::O_RDWR;
use kernel::string::{memset, strlen};
use ulib::{fprintf, gets};
use ulib::stubs::{chdir, close, dup, exec, exit, fork, mknod, open, pipe, read, wait, write};
use crate::CmdType::{BACK, EXEC, LIST, PIPE, REDIR};

// Parsed command representation
#[derive(Copy, Clone)]
enum CmdType {
    EXEC, REDIR, PIPE, LIST, BACK
}

const MAXARGS: usize = 10;
trait Cmd {
    fn get_type(&self) -> CmdType;
    fn run(&self);
    fn nulterminate(&self);
}

struct ExecCmd {
    cmd_type: CmdType,
    argv: [*const u8; MAXARGS],
    eargv: [*const u8; MAXARGS],
}

impl ExecCmd {
    fn new() -> Self {
        Self {
            cmd_type: EXEC,
            argv: [0 as *const u8; MAXARGS],
            eargv: [0 as *const u8; MAXARGS],
        }
    }
}

impl Cmd for ExecCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if self.argv[0] == 0 as *const u8 { 
            unsafe { exit(1); }
        }
        
        unsafe { exec(self.argv[0], self.argv.as_ptr()); }
        fprintf!(2, "exec {} failed\n", self.argv[0]);
    }

    fn nulterminate(&self) {
        todo!()
    }
}

struct RedirCmd<'a> {
    cmd_type: CmdType,
    cmd: &'a dyn Cmd,
    file: &'a [u8],
    efile: &'a [u8],
    mode: i32,
    fd: i32
}

impl RedirCmd {
    fn new(subcmd: &dyn Cmd, file: &[u8], efile: &[u8], mode: i32, fd: i32) -> Self {
        Self {
            cmd_type: REDIR,
            cmd: subcmd,
            file,
            efile,
            mode,
            fd,
        }
    }
}

impl Cmd for RedirCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        unsafe { close(self.fd); }
        if unsafe { open(self.file.as_ptr(), self.mode as u64)} < 0{ 
            fprintf!(2, "open {} failed\n", self.file);
            unsafe { exit(1) };
        }
        
        self.cmd.run();
    }

    fn nulterminate(&self) {
        todo!()
    }
}

struct PipeCmd<'a> {
    cmd_type: CmdType,
    left: &'a dyn Cmd,
    right: &'a dyn Cmd,
}

impl PipeCmd {
    fn new(leftcmd: &dyn Cmd, rightcmd: &dyn Cmd,) -> Self {
        Self {
            cmd_type: PIPE,
            left: leftcmd,
            right: rightcmd,
        }
    }
}

impl Cmd for PipeCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        let p = [0, 0];
        if unsafe { pipe(p.as_ptr()) } < 0 {
            panic!("pipe");
        }
        if fork1() == 0 {
            unsafe {
                close(1);
                dup(p[1]);
                close(p[0]);
                close(p[1]);
            }
            self.left.run();
        }
        if fork1() == 0 {
            unsafe {
                close(0);
                dup(p[0]);
                close(p[0]);
                close(p[1]);
            }
            self.right.run();
        }
        unsafe {
            close(p[0]);
            close(p[1]);
            wait(0 as *const u8);
            wait(0 as *const u8);
        }
    }

    fn nulterminate(&self) {
        todo!()
    }
}

struct ListCmd<'a> {
    cmd_type: CmdType,
    left: &'a dyn Cmd,
    right: &'a dyn Cmd,
}

impl ListCmd {
    fn new(leftcmd: &dyn Cmd, rightcmd: &dyn Cmd) -> Self {
        Self {
            cmd_type: LIST,
            left: leftcmd,
            right: rightcmd,
        }
    }
}

impl Cmd for ListCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if fork1() == 0 {
            self.left.run();
        }
        unsafe { wait(0 as *const u8); }
        self.right.run();
    }

    fn nulterminate(&self) {
        todo!()
    }
}

struct BackCmd<'a> {
    cmd_type: CmdType,
    cmd: &'a dyn Cmd,
}

impl BackCmd {
    fn new(subcmd: &dyn Cmd) -> Self {
        Self {
            cmd_type: BACK,
            cmd: subcmd
        }
    }
}

impl Cmd for BackCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if fork1() == 0 {
            self.cmd.run();
        }
    }

    fn nulterminate(&self) {
        todo!()
    }
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