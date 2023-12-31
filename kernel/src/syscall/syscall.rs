use crate::proc::myproc;

// Retrieve an argument as a pointer.
// Doesn't check for legality, since
// copyin/copyout will do that.
pub(super) fn argaddr(n: u8) -> u64 {
    argraw(n)
}

// Fetch the nth word-sized system call argument as a null-terminated string.
// Copies into buf, at most max.
// Returns string length if OK (including nul), -1 if error.
fn argstr(n: u8, buf: &[char], int max) -> i32 {
    uint64 addr;
    let addr = argaddr(n);
    return fetchstr(addr, buf, max);
}

fn argraw(n: u8) -> u64 {
    let p = myproc();
    let tf = unsafe { p.trapframe.unwrap().as_ref() }.unwrap();
    match n {
        0 => tf.a0,
        1 => tf.a1,
        2 => tf.a2,
        3 => tf.a3,
        4 => tf.a4,
        5 => tf.a5,
        _ => {
            panic!("argraw")
        }
    }
}

// Fetch the nul-terminated string at addr from the current process.
// Returns length of string, not including nul, or -1 for error.
fn fetchstr(uint64 addr, char *buf, int max) {
    struct proc *p = myproc();
    if copyinstr(p->pagetable, buf, addr, max) < 0
        return -1;
    return strlen(buf);
}