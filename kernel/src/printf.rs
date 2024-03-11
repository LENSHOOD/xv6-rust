use core::fmt::{Arguments, Write};
use crate::console::CONSOLE_INSTANCE;
use crate::spinlock::Spinlock;

pub static mut PRINTER: Printer = Printer {
    lock: Spinlock::init_lock("pr"),
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
    locking: bool,
}

impl Printer {
    // Print to the console. only understands %d, %x, %p, %s.
    pub fn printf(self: &mut Self, args: Arguments<'_>) {
        let locking = self.locking;
        if locking {
            self.lock.acquire();
        }

        let _ = unsafe { CONSOLE_INSTANCE.write_fmt(args).unwrap() };

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