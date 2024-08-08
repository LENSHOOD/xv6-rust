#![no_std]
#![feature(start)]

extern crate kernel;

use core::sync::atomic::{AtomicUsize, Ordering};
use kernel::file::fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use kernel::string::strlen;
use ulib::{fprintf, strchr};
use ulib::stubs::{chdir, close, dup, exec, exit, fork, open, pipe, read, wait, write};
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
    fn nulterminate(&mut self);
}

struct ExecCmd {
    cmd_type: CmdType,
    argv: [*const u8; MAXARGS],
    eargv: [usize; MAXARGS],
}

impl ExecCmd {
    fn new() -> Self {
        Self {
            cmd_type: EXEC,
            argv: [0 as *const u8; MAXARGS],
            eargv: [0; MAXARGS],
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
        fprintf!(2, "exec {} failed\n", self.argv[0].as_ref().unwrap());
    }

    fn nulterminate(&mut self) {
        for i in 0..MAXARGS {
            self.argv[self.eargv[i]] = 0 as *const u8;
        }
    }
}

struct RedirCmd<'a> {
    cmd_type: CmdType,
    cmd: &'a mut dyn Cmd,
    file: &'a mut [u8],
    efile: usize,
    mode: i32,
    fd: i32
}

impl<'a> RedirCmd<'a> {
    fn new(subcmd: &'a mut dyn Cmd, file: &'a mut [u8], efile: usize, mode: i32, fd: i32) -> Self {
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

impl<'a> Cmd for RedirCmd<'a> {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        unsafe { close(self.fd); }
        if unsafe { open(self.file.as_ptr(), self.mode as u64)} < 0{ 
            fprintf!(2, "open {:?} failed\n", self.file);
            unsafe { exit(1) };
        }
        
        self.cmd.run();
    }

    fn nulterminate(&mut self) {
        self.cmd.nulterminate();
        self.file[self.efile] = 0;
    }
}

struct PipeCmd<'a> {
    cmd_type: CmdType,
    left: &'a mut dyn Cmd,
    right: &'a mut dyn Cmd,
}

impl<'a> PipeCmd<'a> {
    fn new(leftcmd: &'a mut dyn Cmd, rightcmd: &'a mut dyn Cmd,) -> Self {
        Self {
            cmd_type: PIPE,
            left: leftcmd,
            right: rightcmd,
        }
    }
}

impl<'a> Cmd for PipeCmd<'a> {
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

    fn nulterminate(&mut self) {
        self.left.nulterminate();
        self.right.nulterminate();
    }
}

struct ListCmd<'a> {
    cmd_type: CmdType,
    left: &'a mut dyn Cmd,
    right: &'a mut dyn Cmd,
}

impl<'a> ListCmd<'a> {
    fn new(leftcmd: &'a mut dyn Cmd, rightcmd: &'a mut dyn Cmd) -> Self {
        Self {
            cmd_type: LIST,
            left: leftcmd,
            right: rightcmd,
        }
    }
}

impl<'a> Cmd for ListCmd<'a> {
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

    fn nulterminate(&mut self) {
        self.left.nulterminate();
        self.right.nulterminate();
    }
}

struct BackCmd<'a> {
    cmd_type: CmdType,
    cmd: &'a mut dyn Cmd,
}

impl<'a> BackCmd<'a> {
    fn new(subcmd: &'a mut dyn Cmd) -> Self {
        Self {
            cmd_type: BACK,
            cmd: subcmd
        }
    }
}

impl<'a> Cmd for BackCmd<'a> {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if fork1() == 0 {
            self.cmd.run();
        }
    }

    fn nulterminate(&mut self) {
        self.cmd.nulterminate();
    }
}

struct Cmdline {
    buf: [u8; 100],
    idx: AtomicUsize,
    end: usize
}

impl Cmdline {
    fn new() -> Self {
        Self {
            buf: [0; 100],
            idx: AtomicUsize::new(0),
            end: 0,
        }
    }
    
    fn get_cur(&self) -> u8 {
        self.buf[self.idx.load(Ordering::Relaxed)]
    }
    
    fn gets(&mut self) {
        let mut c: u8 = 0;
        let mut i = 0;
        while i+1 < self.buf.len() {
            let cc = unsafe { read(0, &mut c as *mut u8, 1) };
            if cc < 1 {
                break
            }

            self.buf[i] = c;
            i+=1;
            if c == b'\n' || c == b'\r' {
                break;
            }
        }

        self.buf[i] = b'\0';
    }
    
    fn parsecmd<'a>(&mut self) -> &'a dyn Cmd {
        self.end = strlen(&self.buf as *const u8);
        let cmd = self.parseline();
        self.peek("".as_bytes());
        if self.idx.load(Ordering::Relaxed) != self.end {
            fprintf!(2, "leftovers: {:?}\n", self.buf);
            panic!("syntax");
        }
        cmd.nulterminate();
        return cmd;
    }

    fn parseline(&self) -> &mut dyn Cmd {
        let mut cmd = self.parsepipe();
        while self.peek(&[b'&']) {
            self.gettoken(None, None);
            cmd = &mut BackCmd::new(cmd);
        }
        if self.peek(&[b';']) {
            self.gettoken(None, None);
            cmd = &mut ListCmd::new(cmd, self.parseline());
        }
        return cmd;
    }

    fn parsepipe(&self) -> &mut dyn Cmd {
        let mut cmd = self.parseexec();
        if self.peek(&[b'|']) {
            self.gettoken(None, None);
            cmd = &mut PipeCmd::new(cmd, self.parsepipe());
        }
        return cmd;
    }

    fn parseexec(&self) -> &mut dyn Cmd {
        if self.peek(&[b'(']) {
            return self.parseblock();
        }

        let mut cmd = ExecCmd::new();
        let mut ret: &mut dyn Cmd = &mut cmd;

        let mut argc = 0;
        let mut tok;
        ret = self.parseredirs(ret);
        while !self.peek(&[b'|', b')', b'&', b';']) {
            let mut q = 0;
            let mut eq = 0;
            tok = self.gettoken(Some(&mut q), Some(&mut eq));
            if tok == 0 {
                break;
            } else if tok != b'a' {
                panic!("syntax");
            }

            (&mut cmd).argv[argc] = (&self.buf[q..eq]).as_ptr();
            (&mut cmd).eargv[argc] = eq;
            argc += 1;
            if argc >= MAXARGS {
                panic!("too many args");
            }
            ret = self.parseredirs(ret);
        }
        (&mut cmd).argv[argc] = 0 as *const u8;
        (&mut cmd).eargv[argc] = 0;
        return ret;
    }

    fn parseblock(&self) -> &mut dyn Cmd {
        if !self.peek(&[b'(']) {
            panic!("parseblock");
        }
        self.gettoken(None, None);
        let mut cmd = self.parseline();
        if !self.peek(&[b')']) {
            panic!("syntax - missing )");
        }
        self.gettoken(None, None);
        cmd = self.parseredirs(cmd);
        return cmd;
    }

    fn parseredirs<'a>(&self, cmd: &'a mut dyn Cmd) -> &mut dyn Cmd {
        let mut tok;
        let mut cmd = cmd;
        while self.peek(&[b'<', b'>']) {
            let mut q = 0;
            let mut eq = 0;
            tok = self.gettoken(None, None);
            if self.gettoken(Some(&mut q), Some(&mut eq)) != b'a' {
                panic!("missing file for redirection");
            }
            let file = &self.buf[q..eq];
            cmd = match tok {
                b'<' => &mut RedirCmd::new(cmd, &mut file.clone(), eq, O_RDONLY as i32, 0),
                b'>' => &mut RedirCmd::new(cmd, &mut file.clone(), eq, (O_WRONLY|O_CREATE|O_TRUNC) as i32, 1),
                b'+' => &mut RedirCmd::new(cmd, &mut file.clone(), eq, (O_WRONLY|O_CREATE) as i32, 1),
                _ => cmd
            }
        }
        return cmd;
    }

    const whitespace: [u8; 4] = [b' ', b'\t', b'\r', b'\n'];
    const symbols: [u8; 7] = [b'<', b'|', b'>', b'&', b';', b'(', b')'];
    fn peek(&self, toks: &[u8]) -> bool {
        while self.idx.load(Ordering::Relaxed) < self.end && strchr(&Self::whitespace, self.get_cur()) != 0 {
            self.idx.fetch_add(1, Ordering::Relaxed);
        }
        return self.get_cur() != 0 && strchr(toks, self.get_cur()) != 0;
    }
    fn gettoken(&self, q: Option<&mut usize>, eq: Option<&mut usize>) -> u8 {
        while self.idx.load(Ordering::Relaxed) < self.end && strchr(&Self::whitespace, self.get_cur()) != 0 {
            self.idx.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(q) = q {
            *q = self.idx.load(Ordering::Relaxed);
        }
        let mut ret = self.get_cur();
        match self.get_cur() {
            b'\0' => (),
            b'|' | b'(' | b')' | b';' | b'&' | b'<' => { self.idx.fetch_add(1, Ordering::Relaxed); },
            b'>' => {
                self.idx.fetch_add(1, Ordering::Relaxed);
                if self.get_cur() == b'>' {
                    ret = b'+';
                    self.idx.fetch_add(1, Ordering::Relaxed);
                }
            }
            _ => {
                ret = b'a';
                while self.idx.load(Ordering::Relaxed) < self.end
                    && strchr(&Self::whitespace, self.get_cur()) != 0
                    && !strchr(&Self::symbols, self.get_cur()) != 0 {

                    self.idx.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        if let Some(eq) = eq {
            *eq = self.idx.load(Ordering::Relaxed);
        }

        while self.idx.load(Ordering::Relaxed) < self.end && strchr(&Self::whitespace, self.get_cur()) != 0 {
            self.idx.fetch_add(1, Ordering::Relaxed);
        }
        return ret;
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
    while let Some(mut cmdline) = getcmd() {
        let buf = &mut cmdline.buf;
        if buf[0] == b'c' && buf[1] == b'd' && buf[2] == b' ' {
            // Chdir must be called by the parent, not the child.
            buf[strlen(buf as *const u8)-1] = 0;  // chop \n
            if unsafe { chdir((buf as *mut u8).add(3)) } < 0 {
                fprintf!(2, "cannot cd {:?}\n", &buf[3..]);
            }
            continue;
        }
        if fork1() == 0 {
            cmdline.parsecmd().run();
        }
        unsafe { wait(0 as *const u8); }
    }

    unsafe { exit(0); }
}

fn getcmd() -> Option<Cmdline> {
    unsafe { write(2, "$ \0".as_bytes().as_ptr(), 2); }
    let mut cmdline = Cmdline::new();
    cmdline.gets();
    if cmdline.buf[0] == 0 { // EOF
        return None;
    }
    
    return Some(cmdline);
}

fn fork1() -> i32 {
    let pid = unsafe { fork() };
    if pid == -1 {
        panic!("fork");
    }
    return pid;
}