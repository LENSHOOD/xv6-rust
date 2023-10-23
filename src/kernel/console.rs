use core::fmt::{Error, Write};
use crate::spinlock::Spinlock;
use crate::uart::Uart;

const BACKSPACE: u16 = 0x100;

const INPUT_BUF_SIZE: usize = 128;
pub struct Console {
    lock: Spinlock,
    uart: Uart,

    // input

    buf: [u8; INPUT_BUF_SIZE],
    r: usize,  // Read index
    w: usize,  // Write index
    e: usize,  // Edit index
}

impl Console {
    pub const fn create() -> Self {
        Self {
            lock: Spinlock::init_lock("cons"),
            uart: Uart::create(),
            buf: [0; INPUT_BUF_SIZE],
            r: 0,
            w: 0,
            e: 0,
        }
    }
    pub fn init() {

        // TODO: unimplemented
        // connect read and write system calls
        // to consoleread and consolewrite.
        // devsw[CONSOLE].read = consoleread;
        // devsw[CONSOLE].write = consolewrite;

        Uart::init();
    }

    // send one character to the uart.
    // called by printf(), and to echo input characters,
    // but not from write().
    pub fn putc(self: &mut Self, c: u16) {
        if c == BACKSPACE {
            // if the user typed backspace, overwrite with a space.
            self.uart.putc_sync(0x08); // ascii \b char
            self.uart.putc_sync(0x20); // ascii space char
            self.uart.putc_sync(0x08); // ascii \b char
        } else {
            self.uart.putc_sync(c as u8);
        }
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
