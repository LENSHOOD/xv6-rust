use core::fmt::{Arguments, Write};
use core::mem::swap;
use crate::console::Console;
use crate::spinlock::Spinlock;

pub static mut PRINTER: Option<Printer> = None;

#[macro_export]
macro_rules! printf
{
	($($arg:tt)*) => {
        unsafe {
            if let Some(printer) = &mut crate::printf::PRINTER {
                printer.printf(core::format_args!($($arg)*))
            }
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
    pub fn init(console: Console) -> Self {
        Printer {
            lock: Spinlock::init_lock("pr"),
            console,
            locking: true,
        }
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