use core::arch::asm;

pub fn r_mhartid() -> u64{
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, mhartid", out(reg) x)
    }
    x
}

// Machine Status Register, mstatus
pub const MSTATUS_MPP_MASK: u64 = 3 << 11; // previous mode.
pub const MSTATUS_MPP_M: u64 = 3 << 11;
pub const MSTATUS_MPP_S: u64 = 1 << 11;
pub const MSTATUS_MPP_U: u64 = 0 << 11;
pub const MSTATUS_MIE: u64 = 1 << 3; // machine-mode interrupt enable.


pub fn r_mstatus() -> u64{
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, mstatus", out(reg) x)
    }
    x
}

pub fn w_mstatus(x: u64) {
    unsafe {
        asm!("csrw mstatus, {}", in(reg) x)
    }
}

// machine exception program counter, holds the
// instruction address to which a return from
// exception will go.
pub fn w_mepc(x: usize) {
    unsafe {
        asm!("csrw mepc, {}", in(reg) x)
    }
}

// Supervisor Status Register, sstatus
pub const SSTATUS_SPP: u64 = 1 << 8; // Previous mode, 1=Supervisor, 0=User
pub const SSTATUS_SPIE: u64 = 1 << 5; // Supervisor Previous Interrupt Enable
pub const SSTATUS_UPIE: u64 = 1 << 4; // User Previous Interrupt Enable
pub const SSTATUS_SIE: u64 = 1 << 1; // Supervisor Interrupt Enable
pub const SSTATUS_UIE: u64 = 1 << 0; // User Interrupt Enable

pub fn r_sstatus() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) x)
    }
    x
}

pub fn w_sstatus(x: u64) {
    unsafe {
        asm!("csrw sstatus, {}", in(reg) x)
    }
}

// Supervisor Interrupt Pending
pub fn r_sip() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, sip", out(reg) x)
    }
    x
}

pub fn w_sip(x: u64) {
    unsafe {
        asm!("csrw sip, {}", in(reg) x)
    }
}

// Supervisor Interrupt Enable
pub const SIE_SEIE: u64 = 1 << 9; // external
pub const SIE_STIE: u64 = 1 << 5; // timer
pub const SIE_SSIE: u64 = 1 << 1; // software
pub fn r_sie() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, sie", out(reg) x)
    }
    x
}

pub fn w_sie(x: u64) {
    unsafe {
        asm!("csrw sie, {}", in(reg) x)
    }
}

// Machine-mode Interrupt Enable
pub const MIE_MEIE: u64 = 1 << 11; // external
pub const MIE_MTIE: u64 = 1 << 7; // timer
pub const MIE_MSIE: u64 = 1 << 3; // software
pub fn r_mie() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, mie", out(reg) x)
    }
    x
}

pub fn w_mie(x: u64) {
    unsafe {
        asm!("csrw mie, {}", in(reg) x)
    }
}

// supervisor exception program counter, holds the
// instruction address to which a return from
// exception will go.
pub fn r_sepc() -> usize {
    let mut x: usize = 0;
    unsafe {
        asm!("csrr {}, sepc", out(reg) x)
    }
    x
}

pub fn w_sepc(x: usize) {
    unsafe {
        asm!("csrw sepc, {}", in(reg) x)
    }
}

// Machine Exception Delegation
pub fn r_medeleg() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, medeleg", out(reg) x)
    }
    x
}

pub fn w_medeleg(x: u64) {
    unsafe {
        asm!("csrw medeleg, {}", in(reg) x)
    }
}

// Machine Interrupt Delegation
pub fn r_mideleg() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, mideleg", out(reg) x)
    }
    x
}

pub fn w_mideleg(x: u64) {
    unsafe {
        asm!("csrw mideleg, {}", in(reg) x)
    }
}

// Supervisor Trap-Vector Base Address
// low two bits are mode.
pub fn r_stvec() -> usize {
    let mut x: usize = 0;
    unsafe {
        asm!("csrr {}, stvec", out(reg) x)
    }
    x
}

pub fn w_stvec(x: usize) {
    unsafe {
        asm!("csrw stvec, {}", in(reg) x)
    }
}

// Machine-mode interrupt vector
pub fn w_mtvec(x: usize) {
    unsafe {
        asm!("csrw mtvec, {}", in(reg) x)
    }
}

// Physical Memory Protection
pub fn w_pmpcfg0(x: u64) {
    unsafe {
        asm!("csrw pmpcfg0, {}", in(reg) x)
    }
}

pub fn w_pmpaddr0(x: u64) {
    unsafe {
        asm!("csrw pmpaddr0, {}", in(reg) x)
    }
}

// supervisor address translation and protection;
// holds the address of the page table.
pub fn r_satp() -> usize {
    let mut x: usize = 0;
    unsafe {
        asm!("csrr {}, satp", out(reg) x)
    }
    x
}

// use riscv's sv39 page table scheme.
pub const SATP_SV39: usize = 8 << 60;
#[macro_export]
macro_rules! MAKE_SATP {
    ( $x:expr ) => {
        $crate::riscv::SATP_SV39 | (($x) >> 12)
    };
}

pub fn w_satp(x: usize) {
    unsafe {
        asm!("csrw satp, {}", in(reg) x)
    }
}

pub fn w_mscratch(x: usize) {
    unsafe {
        asm!("csrw mscratch, {}", in(reg) x)
    }
}

// Supervisor Trap Cause
pub fn r_scause() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, scause", out(reg) x)
    }
    x
}

// Supervisor Trap Value
pub fn r_stval() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, stval", out(reg) x)
    }
    x
}

// Machine-mode Counter-Enable
pub fn r_mcounteren() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, mcounteren", out(reg) x)
    }
    x
}

pub fn w_mcounteren(x: u64) {
    unsafe {
        asm!("csrw mcounteren, {}", in(reg) x)
    }
}

// machine-mode cycle counter
pub fn r_time() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("csrr {}, time", out(reg) x)
    }
    x
}

// enable device interrupts
pub fn intr_on() {
    w_sstatus(r_sstatus() | SSTATUS_SIE);
}

// disable device interrupts
pub fn intr_off() {
    w_sstatus(r_sstatus() & !SSTATUS_SIE);
}

// are device interrupts enabled?
pub fn intr_get() -> bool {
    let x = r_sstatus();
    (x & SSTATUS_SIE) != 0
}

pub fn r_sp() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("mv {}, sp", out(reg) x)
    }
    x
}

// read and write tp, the thread pointer, which xv6 uses to hold
// this core's hartid (core number), the index into cpus[].
pub fn r_tp() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("mv {}, tp", out(reg) x)
    }
    x
}

pub fn w_tp(x: u64) {
    unsafe {
        asm!("mv tp, {}", in(reg) x)
    }
}

pub fn r_ra() -> u64 {
    let mut x: u64 = 0;
    unsafe {
        asm!("mv {}, ra", out(reg) x)
    }
    x
}

// flush the TLB.
pub fn sfence_vma() {
    unsafe {
        asm!("sfence.vma zero, zero")
    }
}

pub const PGSIZE: u64 = 4096; // bytes per page
pub const PGSHIFT: u64 = 12;  // bits of offset within a page

#[macro_export]
macro_rules! PGROUNDUP {
    ( $sz:expr ) => {
        (($sz) + $crate::riscv::PGSIZE - 1) & !($crate::riscv::PGSIZE - 1)
    };
}
#[macro_export]
macro_rules! PGROUNDDOWN {
    ( $a:expr ) => {
        (($a)) & !(crate::riscv::PGSIZEPGSIZE - 1)
    };
}

pub const PTE_V: u64 = 1 << 0; // valid
pub const PTE_R: u64 = 1 << 1;
pub const PTE_W: u64 = 1 << 2;
pub const PTE_X: u64 = 1 << 3;
pub const PTE_U: u64 = 1 << 4;// user can access

// shift a physical address to the right place for a PTE.
#[macro_export]
macro_rules! PA2PTE {
    ( $pa:expr ) => {
        (($pa) as u64 >> 12) << 10
    };
}

#[macro_export]
macro_rules! PTE2PA {
    ( $pta:expr ) => {
        (($pta) as u64 >> 10) << 12
    };
}

#[macro_export]
macro_rules! PTE_FLAGS {
    ( $pte:expr ) => {
        ($pte) as u64 & 0x3FF
    };
}

// extract the three 9-bit page table indices from a virtual address.
pub const PXMASK: u64 = 0x1FF; // 9 bits
#[macro_export]
macro_rules! PXSHIFT {
    ( $level:expr ) => {
        crate::riscv::PGSHIFT + (9 * ($level))
    };
}
#[macro_export]
macro_rules! PX {
    ( $level:expr,  $va:expr) => {
        (($va) as u64 >> crate::riscv::PXSHIFT($level)) & crate::riscv::PXMASK
    };
}

// one beyond the highest possible virtual address.
// MAXVA is actually one bit less than the max allowed by
// Sv39, to avoid having to sign-extend virtual addresses
// that have the high bit set.
pub const MAXVA: u64 = 1 << (9 + 9 + 9 + 12 - 1);