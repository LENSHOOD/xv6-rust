use core::fmt::{Arguments, Write};
use crate::console::Console;
use crate::file::CONSOLE;
use crate::spinlock::Spinlock;
use crate::uart::CONSOLE_INSTANCE;

pub static mut PRINTER: Printer = Printer {
    lock: Spinlock::init_lock("pr"),
    console: None,
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
    console: Option<&'static mut Console>,
    locking: bool,
}

impl Printer {
    pub fn init() {
        unsafe {
            CONSOLE_INSTANCE.init();
            PRINTER.console = Some(&mut CONSOLE_INSTANCE); }
    }

    // Print to the console. only understands %d, %x, %p, %s.
    pub fn printf(self: &mut Self, args: Arguments<'_>) {
        let locking = self.locking;
        if locking {
            self.lock.acquire();
        }

        let _ = self.console.as_mut().unwrap().write_fmt(args);

        if locking {
            self.lock.release()
        }
    }
}

#[macro_export]
macro_rules! debug_log {
	($($arg:tt)*) => {
        #[cfg(log_level = "debug")]
        crate::printf!($($arg)*)
    };
}