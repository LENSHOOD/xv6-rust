#![no_std]
#![feature(start)]

extern crate alloc;
extern crate kernel;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use kernel::file::fcntl::{O_CREATE, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use kernel::string::strlen;
use ulib::stubs::{chdir, close, dup, exec, exit, fork, open, pipe, read, wait, write};
use ulib::{fprintf, gets, strchr};

use crate::CmdType::{BACK, EXEC, LIST, PIPE, REDIR};

// Parsed command representation
#[derive(Copy, Clone)]
enum CmdType {
    EXEC,
    REDIR,
    PIPE,
    LIST,
    BACK,
}

const MAXARGS: usize = 10;
trait Cmd {
    fn get_type(&self) -> CmdType;
    fn run(&self);
    fn nulterminate(&mut self);
}

struct ExecCmd {
    cmd_type: CmdType,
    argv: Rc<RefCell<[*mut u8; MAXARGS]>>,
    eargv: Rc<RefCell<[usize; MAXARGS]>>,
}

impl ExecCmd {
    fn new() -> Self {
        Self {
            cmd_type: EXEC,
            argv: Rc::new(RefCell::new([0 as *mut u8; MAXARGS])),
            eargv: Rc::new(RefCell::new([0; MAXARGS])),
        }
    }
}

impl Cmd for ExecCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if self.argv.borrow_mut()[0] == 0 as *mut u8 {
            unsafe {
                exit(1);
            }
        }

        unsafe {
            exec(self.argv.borrow()[0], self.argv.borrow().as_ptr() as *const *const u8);
        }
        fprintf!(
            2,
            "exec {} failed\n",
            self.argv.borrow_mut()[0].as_ref().unwrap()
        );
    }

    fn nulterminate(&mut self) {
        for i in 0..MAXARGS {
            let curr_argv = self.argv.borrow_mut()[i];
            if curr_argv.is_null() {
                break;
            }

            unsafe { curr_argv.add(self.eargv.borrow_mut()[i]).write_volatile(0); }
        }
    }
}

struct RedirCmd {
    cmd_type: CmdType,
    cmd: Rc<RefCell<Box<dyn Cmd>>>,
    file: [u8; CMD_MAX_LEN],
    efile: usize,
    mode: i32,
    fd: i32,
}

impl RedirCmd {
    fn new(
        subcmd: Rc<RefCell<Box<dyn Cmd>>>,
        file: [u8; CMD_MAX_LEN],
        efile: usize,
        mode: i32,
        fd: i32,
    ) -> Self {
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
        unsafe {
            close(self.fd);
        }
        if unsafe { open(self.file.as_ptr(), self.mode as u64) } < 0 {
            fprintf!(2, "open {:?} failed\n", self.file);
            unsafe { exit(1) };
        }

        self.cmd.borrow().run();
    }

    fn nulterminate(&mut self) {
        self.cmd.borrow_mut().nulterminate();
        self.file[self.efile] = 0;
    }
}

struct PipeCmd {
    cmd_type: CmdType,
    left: Rc<RefCell<Box<dyn Cmd>>>,
    right: Rc<RefCell<Box<dyn Cmd>>>,
}

impl PipeCmd {
    fn new(leftcmd: Rc<RefCell<Box<dyn Cmd>>>, rightcmd: Rc<RefCell<Box<dyn Cmd>>>) -> Self {
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
            self.left.borrow().run();
        }
        if fork1() == 0 {
            unsafe {
                close(0);
                dup(p[0]);
                close(p[0]);
                close(p[1]);
            }
            self.right.borrow().run();
        }
        unsafe {
            close(p[0]);
            close(p[1]);
            wait(0 as *const u8);
            wait(0 as *const u8);
        }
    }

    fn nulterminate(&mut self) {
        self.left.borrow_mut().nulterminate();
        self.right.borrow_mut().nulterminate();
    }
}

struct ListCmd {
    cmd_type: CmdType,
    left: Rc<RefCell<Box<dyn Cmd>>>,
    right: Rc<RefCell<Box<dyn Cmd>>>,
}

impl ListCmd {
    fn new(leftcmd: Rc<RefCell<Box<dyn Cmd>>>, rightcmd: Rc<RefCell<Box<dyn Cmd>>>) -> Self {
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
            self.left.borrow().run();
        }
        unsafe {
            wait(0 as *const u8);
        }
        self.right.borrow().run();
    }

    fn nulterminate(&mut self) {
        self.left.borrow_mut().nulterminate();
        self.right.borrow_mut().nulterminate();
    }
}

struct BackCmd {
    cmd_type: CmdType,
    cmd: Rc<RefCell<Box<dyn Cmd>>>,
}

impl BackCmd {
    fn new(subcmd: Rc<RefCell<Box<dyn Cmd>>>) -> Self {
        Self {
            cmd_type: BACK,
            cmd: subcmd,
        }
    }
}

impl Cmd for BackCmd {
    fn get_type(&self) -> CmdType {
        self.cmd_type
    }

    fn run(&self) {
        if fork1() == 0 {
            self.cmd.borrow().run();
        }
    }

    fn nulterminate(&mut self) {
        self.cmd.borrow_mut().nulterminate();
    }
}

const CMD_MAX_LEN: usize = 100;
struct Cmdline {
    buf: [u8; CMD_MAX_LEN],
    idx: AtomicUsize,
    end: usize,
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
        self.buf[self.get_idx()]
    }

    fn idx_add_1(&self) -> usize {
        self.idx.fetch_add(1, Ordering::Relaxed)
    }

    fn get_idx(&self) -> usize {
        self.idx.load(Ordering::Relaxed)
    }

    fn gets(&mut self) {
        let len = self.buf.len();
        gets(&mut self.buf, len);
    }

    fn parsecmd(&mut self) -> Rc<RefCell<Box<dyn Cmd>>> {
        self.end = strlen(&self.buf as *const u8);
        let cmd = self.parseline();
        self.peek("".as_bytes());
        if self.get_idx() != self.end {
            fprintf!(2, "leftovers: {:?}\n", self.buf);
            panic!("syntax");
        }
        cmd.borrow_mut().nulterminate();
        return cmd;
    }

    fn parseline(&mut self) -> Rc<RefCell<Box<dyn Cmd>>> {
        let mut cmd = self.parsepipe();
        while self.peek(&[b'&']) {
            self.gettoken(None, None);
            cmd = Rc::new(RefCell::new(Box::new(BackCmd::new(cmd.clone()))));
        }
        if self.peek(&[b';']) {
            self.gettoken(None, None);
            cmd = Rc::new(RefCell::new(Box::new(ListCmd::new(
                cmd.clone(),
                self.parseline(),
            ))));
        }
        return cmd;
    }

    fn parsepipe(&mut self) -> Rc<RefCell<Box<dyn Cmd>>> {
        let mut cmd = self.parseexec();
        if self.peek(&[b'|']) {
            self.gettoken(None, None);
            cmd = Rc::new(RefCell::new(Box::new(PipeCmd::new(
                cmd.clone(),
                self.parsepipe().clone(),
            ))));
        }
        return cmd;
    }

    fn parseexec(&mut self) -> Rc<RefCell<Box<dyn Cmd>>> {
        if self.peek(&[b'(']) {
            return self.parseblock();
        }

        let exec_md = ExecCmd::new();
        let argv = exec_md.argv.clone();
        let eargv = exec_md.eargv.clone();
        let cmd = Rc::new(RefCell::new(Box::new(exec_md) as Box<dyn Cmd>));

        let mut argc = 0;
        let mut tok;
        let mut ret = self.parseredirs(cmd);
        while !self.peek(&[b'|', b')', b'&', b';']) {
            let mut q = 0;
            let mut eq = 0;
            tok = self.gettoken(Some(&mut q), Some(&mut eq));
            if tok == 0 {
                break;
            } else if tok != b'a' {
                panic!("syntax");
            }

            argv.borrow_mut()[argc] = (&mut self.buf[q..]).as_mut_ptr();
            // calculate which index of the current argv should be set to null eventually
            eargv.borrow_mut()[argc] = eq - q;
            argc += 1;
            if argc >= MAXARGS {
                panic!("too many args");
            }
            ret = self.parseredirs(ret);
        }

        argv.borrow_mut()[argc] = 0 as *mut u8;
        eargv.borrow_mut()[argc] = 0;
        return ret;
    }

    fn parseblock(&mut self) -> Rc<RefCell<Box<dyn Cmd>>> {
        if !self.peek(&[b'(']) {
            panic!("parseblock");
        }
        self.gettoken(None, None);
        let cmd = self.parseline();
        if !self.peek(&[b')']) {
            panic!("syntax - missing )");
        }
        self.gettoken(None, None);
        return self.parseredirs(cmd);
    }

    fn parseredirs(&self, cmd: Rc<RefCell<Box<dyn Cmd>>>) -> Rc<RefCell<Box<dyn Cmd>>> {
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
            let mut new_file = [0; CMD_MAX_LEN];
            new_file[..file.len()].copy_from_slice(file);
            cmd = match tok {
                b'<' => Rc::new(RefCell::new(Box::new(RedirCmd::new(
                    cmd,
                    new_file,
                    eq,
                    O_RDONLY as i32,
                    0,
                )))),
                b'>' => Rc::new(RefCell::new(Box::new(RedirCmd::new(
                    cmd,
                    new_file,
                    eq,
                    (O_WRONLY | O_CREATE | O_TRUNC) as i32,
                    1,
                )))),
                b'+' => Rc::new(RefCell::new(Box::new(RedirCmd::new(
                    cmd,
                    new_file,
                    eq,
                    (O_WRONLY | O_CREATE) as i32,
                    1,
                )))),
                _ => cmd,
            }
        }
        return cmd;
    }

    const WHITESPACE: [u8; 4] = [b' ', b'\t', b'\r', b'\n'];
    const SYMBOLS: [u8; 7] = [b'<', b'|', b'>', b'&', b';', b'(', b')'];
    fn peek(&self, toks: &[u8]) -> bool {
        while self.get_idx() < self.end && strchr(&Self::WHITESPACE, self.get_cur()) != 0 {
            self.idx_add_1();
        }
        return self.get_cur() != 0 && strchr(toks, self.get_cur()) != 0;
    }

    fn gettoken(&self, q: Option<&mut usize>, eq: Option<&mut usize>) -> u8 {
        while self.get_idx() < self.end && strchr(&Self::WHITESPACE, self.get_cur()) != 0 {
            self.idx_add_1();
        }
        if let Some(q) = q {
            *q = self.get_idx();
        }
        let mut ret = self.get_cur();
        match self.get_cur() {
            b'\0' => (),
            b'|' | b'(' | b')' | b';' | b'&' | b'<' => {
                self.idx_add_1();
            }
            b'>' => {
                self.idx_add_1();
                if self.get_cur() == b'>' {
                    ret = b'+';
                    self.idx_add_1();
                }
            }
            _ => {
                ret = b'a';
                while self.get_idx() < self.end
                    && strchr(&Self::WHITESPACE, self.get_cur()) == 0
                    && strchr(&Self::SYMBOLS, self.get_cur()) == 0
                {
                    self.idx_add_1();
                }
            }
        }

        if let Some(eq) = eq {
            *eq = self.get_idx();
        }

        while self.get_idx() < self.end && strchr(&Self::WHITESPACE, self.get_cur()) != 0 {
            self.idx_add_1();
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
            unsafe {
                close(fd);
            }
            break;
        }
    }

    // Read and run input commands.
    while let Some(mut cmdline) = getcmd() {
        let buf = &mut cmdline.buf;
        if buf[0] == b'c' && buf[1] == b'd' && buf[2] == b' ' {
            // Chdir must be called by the parent, not the child.
            buf[strlen(buf as *const u8) - 1] = 0; // chop \n
            if unsafe { chdir((buf as *mut u8).add(3)) } < 0 {
                fprintf!(2, "cannot cd {:?}\n", &buf[3..]);
            }
            continue;
        }
        if fork1() == 0 {
            cmdline.parsecmd().borrow().run();
        }
        unsafe {
            wait(0 as *const u8);
        }
    }

    unsafe {
        exit(0);
    }
}

fn getcmd() -> Option<Cmdline> {
    unsafe {
        write(2, "$ \0".as_bytes().as_ptr(), 2);
    }
    let mut cmdline = Cmdline::new();
    cmdline.gets();
    if cmdline.buf[0] == 0 {
        // EOF
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
