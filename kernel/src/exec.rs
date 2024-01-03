use core::mem;
use crate::elf::{ELF_MAGIC, ELF_PROG_LOAD, ElfHeader, ProgramHeader};
use crate::fs::fs::namei;
use crate::log::{begin_op, end_op};
use crate::param::{MAXARG, MAXPATH};
use crate::proc::{myproc, proc_pagetable};
use crate::riscv::{PGSIZE, PTE_W, PTE_X};
use crate::vm::uvmalloc;

fn flags2perm(flags: u32) -> usize {
    let mut perm = 0;
    if flags & 0x1 {
        perm = PTE_X;
    }
    if flags & 0x2 {
        perm |= PTE_W;
    }
    return perm;
}

fn exec(path: &[char; MAXPATH], argv: &[Option<&mut [char]>; MAXARG]) -> i32 {
    let p = myproc();

    begin_op();

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
        goto bad;
    }

    if elf.magic != ELF_MAGIC {
        goto bad;
    }

    let mut page_table_op = proc_pagetable(p);
    if page_table_op.is_none() {
        goto bad;
    }
    let mut page_table = page_table_op.unwrap();

    // Load program into memory.
    let mut off = elf.phoff as u32;
    let mut ph = ProgramHeader::create();
    let ph_sz = mem::size_of::<ProgramHeader>();
    let mut sz = 0;
    for i in 0..elf.phnum {
        let tot = ip.readi(false, &mut ph, off, ph_sz);
        if tot != ph_sz {
            goto bad;
        }
        if ph.hdr_type != ELF_PROG_LOAD {
            continue;
        }
        if ph.memsz < ph.filesz {
            goto bad;
        }
        if ph.vaddr + ph.memsz < ph.vaddr {
            goto bad;
        }
        if ph.vaddr % PGSIZE != 0 {
            goto bad;
        }

        let sz1 = uvmalloc(&mut page_table, sz, (ph.vaddr + ph.memsz) as usize, flags2perm(ph.flags));
        if sz1 == 0 {
            goto bad;
        }
        sz = sz1;
        if loadseg(pagetable, ph.vaddr, ip, ph.off, ph.filesz) < 0 {
            goto bad;
        }

        off += ph_sz;
    }

    for(i=0, off=elf.phoff; i<elf.phnum; i++, off+=sizeof(ph)){


        uint64 sz1;
        if((sz1 = ) == 0)
            goto bad;
        sz = sz1;
        if(loadseg(pagetable, ph.vaddr, ip, ph.off, ph.filesz) < 0)
            goto bad;
    }
    iunlockput(ip);
    end_op();
    ip = 0;

    p = myproc();
    uint64 oldsz = p->sz;

    // Allocate two pages at the next page boundary.
    // Make the first inaccessible as a stack guard.
    // Use the second as the user stack.
    sz = PGROUNDUP(sz);
    uint64 sz1;
    if((sz1 = uvmalloc(pagetable, sz, sz + 2*PGSIZE, PTE_W)) == 0)
        goto bad;
    sz = sz1;
    uvmclear(pagetable, sz-2*PGSIZE);
    sp = sz;
    stackbase = sp - PGSIZE;

    // Push argument strings, prepare rest of stack in ustack.
    for(argc = 0; argv[argc]; argc++) {
        if(argc >= MAXARG)
            goto bad;
        sp -= strlen(argv[argc]) + 1;
        sp -= sp % 16; // riscv sp must be 16-byte aligned
        if(sp < stackbase)
            goto bad;
        if(copyout(pagetable, sp, argv[argc], strlen(argv[argc]) + 1) < 0)
            goto bad;
        ustack[argc] = sp;
    }
    ustack[argc] = 0;

    // push the array of argv[] pointers.
    sp -= (argc+1) * sizeof(uint64);
    sp -= sp % 16;
    if(sp < stackbase)
        goto bad;
    if(copyout(pagetable, sp, (char *)ustack, (argc+1)*sizeof(uint64)) < 0)
        goto bad;

    // arguments to user main(argc, argv)
    // argc is returned via the system call return
    // value, which goes in a0.
    p->trapframe->a1 = sp;

    // Save program name for debugging.
    for(last=s=path; *s; s++)
        if(*s == '/')
            last = s+1;
    safestrcpy(p->name, last, sizeof(p->name));

    // Commit to the user image.
    oldpagetable = p->pagetable;
    p->pagetable = pagetable;
    p->sz = sz;
    p->trapframe->epc = elf.entry;  // initial program counter = main
    p->trapframe->sp = sp; // initial stack pointer
    proc_freepagetable(oldpagetable, oldsz);

    return argc; // this ends up in a0, the first argument to main(argc, argv)

    bad:
        if(pagetable)
            proc_freepagetable(pagetable, sz);
        if(ip){
            iunlockput(ip);
            end_op();
        }
    return -1;
}

// Load a program segment into pagetable at virtual address va.
// va must be page-aligned
// and the pages from va to va+sz must already be mapped.
// Returns 0 on success, -1 on failure.
fn loadseg(pagetable_t pagetable, uint64 va, struct inode *ip, uint offset, uint sz) -> i32 {
    uint i, n;
    uint64 pa;

    for(i = 0; i < sz; i += PGSIZE){
    pa = walkaddr(pagetable, va + i);
    if(pa == 0)
    panic("loadseg: address should exist");
    if(sz - i < PGSIZE)
    n = sz - i;
    else
    n = PGSIZE;
    if(readi(ip, 0, (uint64)pa, offset+i, n) != n)
    return -1;
}

return 0;
}
