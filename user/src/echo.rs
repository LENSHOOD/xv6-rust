#![no_std]
#![feature(start)]

use core::mem;

use ulib::stubs::write;

#[start]
fn main(argc: isize, argv: *const *const u8) -> isize {
    unsafe {
        let args = core::slice::from_raw_parts(argv, argc as usize);

        for i in 1..argc {
            let sz = mem::size_of_val(args[i as usize].as_ref().unwrap());
            write(1, args[i as usize], sz as i32);
            if i + 1 < argc {
                write(1, &(' ' as u8) as *const u8, 1);
            } else {
                write(1, &('\n' as u8) as *const u8, 1);
            }
        }
    }

    0
}
