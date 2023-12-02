use crate::file::File;
use crate::param::NFILE;
use crate::spinlock::Spinlock;

struct FTable<'a> {
    lock: Spinlock,
    file: [File<'a>; NFILE]
}

static mut FTABLE: FTable = FTable {
    lock: Spinlock::init_lock("ftable"),
    file: [File::create(); NFILE],
};

pub fn fileinit() {
    // empty due to FTABLE has already been initialized
}