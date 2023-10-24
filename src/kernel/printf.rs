use core::fmt::{Arguments, Write};
use crate::console::Console;
use crate::spinlock::Spinlock;

pub static mut PRINTER: Printer = Printer {
    lock: Spinlock::init_lock("pr"),
    console: Console::create(),
    locking: true,
};

#[macro_export]
macro_rules! printf
{
	($($arg:tt)*) => {
        unsafe {
            crate::printf::PRINTER.printf(core::format_args!($($arg)*))
        }
    };
}

/// lock to avoid interleaving concurrent printf's.
pub struct Printer {
    lock: Spinlock,
    console: Console,
    locking: bool,
}

impl Printer {
    pub fn init() {
        unsafe { PRINTER.console.init(); }
    }

    // Print to the console. only understands %d, %x, %p, %s.
    pub fn printf(self: &mut Self, args: Arguments<'_>) {
        let locking = self.locking;
        if locking {
            self.lock.acquire();
        }

        let _ = self.console.write_fmt(args);

        if locking {
            self.lock.release()
        }
    }
}

#[macro_export]
macro_rules! debug_log {
	($($arg:tt)*) => {
        #[cfg(log_level = "debug")]
        printf!($($arg)*)
    };
}