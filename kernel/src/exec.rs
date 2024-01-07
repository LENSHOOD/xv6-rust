use alloc::string::String;
use core::mem;
use crate::elf::{ELF_MAGIC, ELF_PROG_LOAD, ElfHeader, ProgramHeader};
use crate::file::INode;
use crate::fs::fs::namei;
use crate::log::{begin_op, end_op};
use crate::param::{MAXARG, MAXPATH};
use crate::PGROUNDUP;
use crate::proc::{myproc, proc_freepagetable, proc_pagetable};
use crate::riscv::{PageTable, PGSIZE, PTE_W, PTE_X};
use crate::string::strlen;
use crate::vm::{copyout, uvmalloc, uvmclear, walkaddr};

fn flags2perm(flags: u32) -> usize {
    let mut perm = 0;
    if flags & 0x1 != 0 {
        perm = PTE_X;
    }
    if flags & 0x2 != 0 {
        perm |= PTE_W;
    }
    return perm;
}

pub fn exec(path: &[u8; MAXPATH], argv: &[Option<*mut u8>; MAXARG]) -> i32 {
    let p = myproc();

    begin_op();

    let path = unsafe { core::str::from_utf8_unchecked(path) };
    let ip_op = namei(path);
    if ip_op.is_none() {
        end_op();
        return -1;
    }
    let ip = ip_op.unwrap();

    ip.ilock();

    // Check ELF header
    let mut elf = ElfHeader::create();
    let tot = ip.readi(false, &mut elf, 0, mem::size_of::<ElfHeader>());
    if tot != mem::size_of::<ElfHeader>() {
        return goto_bad(None, 0, Some(ip));;
    }

    if elf.magic != ELF_MAGIC {
        return goto_bad(None, 0, Some(ip));;
    }

    let mut page_table_op = proc_pagetable(p);
    if page_table_op.is_none() {
        return goto_bad(None, 0, Some(ip));;
    }
    let page_table = unsafe { page_table_op.unwrap().as_mut().unwrap() };

    // Load program into memory.
    let mut off = elf.phoff as u32;
    let mut ph = ProgramHeader::create();
    let ph_sz = mem::size_of::<ProgramHeader>();
    let mut sz = 0;
    for i in 0..elf.phnum {
        let tot = ip.readi(false, &mut ph, off, ph_sz);
        if tot != ph_sz {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }
        if ph.hdr_type != ELF_PROG_LOAD {
            continue;
        }
        if ph.memsz < ph.filesz {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }
        if ph.vaddr + ph.memsz < ph.vaddr {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }
        if ph.vaddr % PGSIZE as u64 != 0 {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }

        let sz1 = uvmalloc(page_table, sz, (ph.vaddr + ph.memsz) as usize, flags2perm(ph.flags));
        if sz1 == 0 {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }
        sz = sz1;
        if loadseg(page_table, ph.vaddr, ip, ph.off, ph.filesz) < 0 {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }

        off += ph_sz as u32;
    }
    ip.iunlockput();
    end_op();

    let p = myproc();
    let oldsz = p.sz;

    // Allocate two pages at the next page boundary.
    // Make the first inaccessible as a stack guard.
    // Use the second as the user stack.
    sz = PGROUNDUP!(sz);
    let sz1 = uvmalloc(page_table, sz, sz + 2*PGSIZE, PTE_W);
    if sz1 == 0 {
        return goto_bad(Some(page_table), sz, Some(ip));;
    }
    sz = sz1;
    uvmclear(page_table, sz-2*PGSIZE);

    let mut sp = sz;
    let stackbase = sp - PGSIZE;
    let argc = 0;
    let mut ustack: [usize; MAXARG] = [0; MAXARG];
    // Push argument strings, prepare rest of stack in ustack.
    loop {
        if argv[argc].is_none() {
            break
        }
        let curr_argv = argv[argc].unwrap();

        if argc >= MAXARG {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }

        sp -= strlen(curr_argv) + 1;
        sp -= sp % 16; // riscv sp must be 16-byte aligned
        if sp < stackbase {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }

        if copyout(page_table, sp, curr_argv, strlen(curr_argv) + 1) < 0 {
            return goto_bad(Some(page_table), sz, Some(ip));;
        }
        ustack[argc] = sp;
    }

    ustack[argc] = 0;

    // push the array of argv[] pointers.
    sp -= (argc+1) * mem::size_of::<u64>();
    sp -= sp % 16;
    if sp < stackbase {
        return goto_bad(Some(page_table), sz, Some(ip));;
    }
    if copyout(page_table, sp, &ustack as *const usize as *const u8, (argc+1)*mem::size_of::<u64>()) < 0 {
        return goto_bad(Some(page_table), sz, Some(ip));;
    }

    // arguments to user main(argc, argv)
    // argc is returned via the system call return
    // value, which goes in a0.
    let tf = unsafe { p.trapframe.unwrap().as_mut().unwrap() };
    tf.a1 = sp as u64;

    // Save program name for debugging.
    let mut name = [0; 16];
    name.copy_from_slice(path.as_bytes());
    p.name = name;

    // Commit to the user image.
    let oldpagetable = unsafe { p.pagetable.unwrap().as_mut().unwrap() };
    p.pagetable = Some(page_table as *mut PageTable);
    p.sz = sz;
    tf.epc = elf.entry;  // initial program counter = main
    tf.sp = sp as u64; // initial stack pointer
    proc_freepagetable(oldpagetable, oldsz);

    return argc as i32; // this ends up in a0, the first argument to main(argc, argv)
}

fn goto_bad(page_table: Option<&mut PageTable>, sz: usize, ip: Option<&mut INode>) -> i32 {
    if page_table.is_none() {
        proc_freepagetable(page_table.unwrap(), sz);
    }

    if ip.is_some() {
        ip.unwrap().iunlockput();
        end_op();
    }

    return -1;
}

// Load a program segment into pagetable at virtual address va.
// va must be page-aligned
// and the pages from va to va+sz must already be mapped.
// Returns 0 on success, -1 on failure.
fn loadseg(page_table: &mut PageTable, va: u64, ip: &mut INode, offset: u64, sz: u64) -> i32 {
    let mut pa = 0;
    let mut n = 0;
    for i in (0..sz).step_by(PGSIZE) {
        let pa_op = walkaddr(page_table, (va + i) as usize);
        if pa_op.is_none() {
            panic!("loadseg: address should exist");
        }
        pa = pa_op.unwrap();

        if sz - i < PGSIZE as u64 {
            n = (sz - i) as usize;
        } else {
            n = PGSIZE;
        }

        if ip.readi(false, pa as *mut u8, (offset + i) as u32, n) != n {
            return -1;
        }
    }

    return 0;
}
