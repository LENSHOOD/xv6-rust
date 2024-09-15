use core::fmt::{Error, Write};

use crate::file::{CONSOLE, Devsw, DEVSW};
use crate::proc::{either_copyin, either_copyout, myproc, procdump, sleep, wakeup};
use crate::spinlock::Spinlock;
use crate::uart::UART_INSTANCE;

pub(crate) static mut CONSOLE_INSTANCE: Console = Console::create();

const BACKSPACE: u16 = 0x100;

const INPUT_BUF_SIZE: usize = 128;
pub struct Console {
    lock: Spinlock,
    // input
    buf: [u8; INPUT_BUF_SIZE],
    r: usize, // Read index
    w: usize, // Write index
    e: usize, // Edit index
}

impl Console {
    pub const fn create() -> Self {
        Self {
            lock: Spinlock::init_lock("cons"),
            buf: [0; INPUT_BUF_SIZE],
            r: 0,
            w: 0,
            e: 0,
        }
    }
    pub fn init() {
        // connect read and write system calls
        // to consoleread and consolewrite.
        unsafe {
            DEVSW[CONSOLE] = Some(&mut CONSOLE_INSTANCE as *mut Console);
        }
    }

    // send one character to the uart.
    // called by printf(), and to echo input characters,
    // but not from write().
    pub fn putc(self: &mut Self, c: u16) {
        unsafe {
            if c == BACKSPACE {
                // if the user typed backspace, overwrite with a space.
                UART_INSTANCE.putc_sync(0x08); // ascii \b char
                UART_INSTANCE.putc_sync(0x20); // ascii space char
                UART_INSTANCE.putc_sync(0x08); // ascii \b char
            } else {
                UART_INSTANCE.putc_sync(c as u8);
            }
        }
    }

    //
    // the console input interrupt handler.
    // uartintr() calls this for input character.
    // do erase/kill processing, append to cons.buf,
    // wake up consoleread() if a whole line has arrived.
    //
    pub(crate) fn consoleintr(self: &mut Self, c: u8) {
        self.lock.acquire();

        match c as char {
            // Print process list.
            'P' => procdump(),
            // Kill line.
            'U' => {
                while self.e != self.w && self.buf[(self.e - 1) % INPUT_BUF_SIZE] != '\n' as u8 {
                    self.e -= 1;
                    self.putc(BACKSPACE);
                }
            }
            // Backspace | Delete key
            'H' | '\x7f' => {
                if self.e != self.w {
                    self.e -= 1;
                    self.putc(BACKSPACE);
                }
            }
            _ => {
                if c != 0 && self.e - self.r < INPUT_BUF_SIZE {
                    let c = if c as char == '\r' { '\n' as u8 } else { c };

                    // echo back to the user.
                    self.putc(c as u16);

                    // store for consumption by consoleread().
                    self.e += 1;
                    self.buf[self.e % INPUT_BUF_SIZE] = c;

                    if c as char == '\n' || c as char == 'D' || self.e - self.r == INPUT_BUF_SIZE {
                        // wake up consoleread() if a whole line (or end-of-file)
                        // has arrived.
                        self.w = self.e;
                        wakeup(&self.r);
                    }
                }
            }
        }

        self.lock.release();
    }
}

impl Write for Console {
    // The trait Write expects us to write the function write_str
    // which looks like:
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            self.putc(c as u16);
        }
        // Return that we succeeded.
        Ok(())
    }
}

impl Devsw for Console {
    //
    // user read()s from the console go here.
    // copy (up to) a whole input line to dst.
    // user_dist indicates whether dst is a user
    // or kernel address.
    //
    fn read(self: &mut Self, is_user_dst: bool, dst: usize, sz: usize) -> i32 {
        let mut c = 0;
        let target = sz;
        let mut cbuf = 0;
        let mut dst = dst;
        let mut sz = sz;

        self.lock.acquire();
        while sz > 0 {
            // wait until interrupt handler has put some
            // input into cons.buffer.
            while self.r == self.w {
                if myproc().killed() != 0 {
                    self.lock.release();
                    return -1;
                }
                sleep(self, &mut self.lock);
            }

            self.r += 1;
            c = self.buf[self.r % INPUT_BUF_SIZE];

            if c as char == 'D' {
                // end-of-file
                if sz < target {
                    // Save ^D for next time, to make sure
                    // caller gets a 0-byte result.
                    self.r -= 1;
                }
                break;
            }

            // copy the input byte to the user-space buffer.
            cbuf = c;
            if either_copyout(is_user_dst, dst as *mut u8, &cbuf, 1) == -1 {
                break;
            }

            dst += 1;
            sz -= 1;

            if c as char == '\n' {
                // a whole line has arrived, return to
                // the user-level read().
                break;
            }
        }
        self.lock.release();

        return (target - sz) as i32;
    }

    fn write(self: &mut Self, is_user_src: bool, src: usize, sz: usize) -> i32 {
        let mut cnt = 0;
        for i in 0..sz {
            let mut c = 0u8;
            if either_copyin(&mut c as *mut u8, is_user_src, src as *const u8, 1) == -1 {
                break;
            }
            self.putc(c as u16);
            cnt = i;
        }

        return cnt as i32;
    }
}
