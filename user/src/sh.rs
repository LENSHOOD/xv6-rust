#![no_std]
#![feature(start)]

extern crate kernel;

use kernel::file::fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use kernel::string::{memset, strlen};
use ulib::{fprintf, gets, strchr};
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

#[derive(Copy, Clone)]
struct Cmdline {
    buf: [u8; 100],
    idx: usize,
    end: usize
}

impl Cmdline {
    fn new() -> Self {
        Self {
            buf: [0; 100],
            idx: 0,
            end: 0,
        }
    }
    
    fn get_cur(&self) -> u8 {
        self.buf[self.idx]
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
            parsecmd(cmdline).run();
        }
        unsafe { wait(0 as *const u8); }
    }

    unsafe { exit(0); }
}

fn getcmd() -> Option<Cmdline> {
    unsafe { write(2, "$ \0".as_bytes().as_ptr(), 2); }
    let mut cmdline = Cmdline::new();
    gets(&mut cmdline.buf as *mut u8, cmdline.buf.len());
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

fn parsecmd<'a>(mut cmdline: Cmdline) -> &'a dyn Cmd {
    cmdline.end = strlen(&cmdline.buf as *const u8);
    let cmd = parseline(&mut cmdline);
    peek(&mut cmdline, "".as_bytes());
    if cmdline.idx != cmdline.end {
        fprintf!(2, "leftovers: {:?}\n", cmdline.buf);
        panic!("syntax");
    }
    cmd.nulterminate();
    return cmd;
}

fn parseline(cmdline: &mut Cmdline) -> &mut dyn Cmd {
    let mut cmd = parsepipe(cmdline);
    while peek(cmdline,&[b'&']) {
        gettoken(cmdline, None, None);
        cmd = &mut BackCmd::new(cmd);
    }
    if peek(cmdline, &[b';']) {
        gettoken(cmdline, None, None);
        cmd = &mut ListCmd::new(cmd, parseline(cmdline));
    }
    return cmd;
}

fn parsepipe(cmdline: &mut Cmdline) -> &mut dyn Cmd {
    let mut cmd = parseexec(cmdline);
    if peek(cmdline, &[b'|']) {
        gettoken(cmdline, None, None);
        cmd = &mut PipeCmd::new(cmd, parsepipe(cmdline));
    }
    return cmd;
}

fn parseexec(cmdline: &mut Cmdline) -> &mut dyn Cmd {
    if peek(cmdline, &[b'(']) {
        return parseblock(cmdline);
    }
    
    let mut cmd = ExecCmd::new();
    let mut ret: &mut dyn Cmd = &mut cmd;
    
    let mut argc = 0;
    let mut tok = 0;
    ret = parseredirs(ret, cmdline);
    while !peek(cmdline, &[b'|', b')', b'&', b';']) {
        let mut q = 0;
        let mut eq = 0;
        tok = gettoken(cmdline, Some(&mut q), Some(&mut eq));
        if tok == 0 {
            break;
        } else if tok != b'a' {
            panic!("syntax");
        }
        (&mut cmd).argv[argc] = (&cmdline.buf[q..eq]).as_ptr();
        (&mut cmd).eargv[argc] = eq;
        argc += 1;
        if argc >= MAXARGS {
            panic!("too many args");
        }
        ret = parseredirs(ret, cmdline);
    }
    (&mut cmd).argv[argc] = 0 as *const u8;
    (&mut cmd).eargv[argc] = 0;
    return ret;
}

fn parseblock(cmdline: &mut Cmdline) -> &mut dyn Cmd {
    if !peek(cmdline, &[b'(']) {
        panic!("parseblock");
    }
    gettoken(cmdline, None, None);
    let mut cmd = parseline(cmdline);
    if !peek(cmdline, &[b')']) {
        panic!("syntax - missing )");
    }
    gettoken(cmdline, None, None);
    cmd = parseredirs(cmd, cmdline);
    return cmd;
}

fn parseredirs<'a>(cmd: &'a mut dyn Cmd, cmdline: &'a mut Cmdline) -> &mut dyn Cmd {
    let mut tok = 0;
    let mut cmd = cmd;
    while peek(cmdline, &[b'<', b'>']) {
        let mut q = 0;
        let mut eq = 0;
        tok = gettoken(cmdline, None, None);
        if gettoken(cmdline, Some(&mut q), Some(&mut eq)) != b'a' {
            panic!("missing file for redirection");
        }
        let file = &mut cmdline.buf[q..eq];
        cmd = match tok { 
            b'<' => &mut RedirCmd::new(cmd, file, eq, O_RDONLY as i32, 0),
            b'>' => &mut RedirCmd::new(cmd, file, eq, (O_WRONLY|O_CREATE|O_TRUNC) as i32, 1),
            b'+' => &mut RedirCmd::new(cmd, file, eq, (O_WRONLY|O_CREATE) as i32, 1),
            _ => cmd 
        }
    }
    return cmd;
}

fn gettoken(cmdline: &mut Cmdline, q: Option<&mut usize>, mut eq: Option<&mut usize>) -> u8 {
    while cmdline.idx < cmdline.end && strchr(&whitespace, cmdline.get_cur()) != 0 {
        cmdline.idx += 1;
    }
    if let Some(q) = q {
        *q = cmdline.idx;
    }
    let mut ret = cmdline.get_cur();
    match cmdline.get_cur() { 
        b'\0' => (),
        b'|' | b'(' | b')' | b';' | b'&' | b'<' => cmdline.idx += 1,
        b'>' => {
            cmdline.idx += 1;
            if cmdline.get_cur() == b'>' {
                ret = b'+';
                cmdline.idx += 1;
            }
        }
        _ => {
            ret = b'a';
            while cmdline.idx < cmdline.end 
                && strchr(&whitespace, cmdline.get_cur()) != 0 
                && !strchr(&symbols, cmdline.get_cur()) != 0 {
                
                cmdline.idx += 1;
            }
        } 
    }
    
    if let Some(mut eq) = eq { 
        *eq = cmdline.idx;
    }
    
    while cmdline.idx < cmdline.end && strchr(&whitespace, cmdline.get_cur()) != 0 {
        cmdline.idx += 1;
    }
    return ret;
}

const whitespace: [u8; 4] = [b' ', b'\t', b'\r', b'\n'];
const symbols: [u8; 7] = [b'<', b'|', b'>', b'&', b';', b'(', b')'];
fn peek(cmdline: &mut Cmdline, toks: &[u8]) -> bool {
    let s = &cmdline.buf;
    while cmdline.idx < cmdline.end && strchr(&whitespace, cmdline.get_cur()) != 0 {
        cmdline.idx += 1;
    }
    return cmdline.get_cur() != 0 && strchr(toks, cmdline.get_cur()) != 0;
}
