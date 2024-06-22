//
// the riscv Platform Level Interrupt Controller (PLIC).
//

use crate::memlayout::{PLIC, UART0_IRQ, VIRTIO0_IRQ};
use crate::proc::cpuid;
use crate::{proc, PLIC_SCLAIM, PLIC_SENABLE, PLIC_SPRIORITY};

pub(crate) fn plicinit() {
    unsafe {
        // set desired IRQ priorities non-zero (otherwise disabled).
        let uart_irq_ref = (PLIC + UART0_IRQ * 4) as *mut u32;
        uart_irq_ref.write_volatile(1);
        let virtio_irq_ref = ((PLIC + VIRTIO0_IRQ * 4) as *mut u32);
        virtio_irq_ref.write_volatile(1);
    }
}

pub(crate) fn plicinithart() {
    let hart = cpuid();

    unsafe {
        // set enable bits for this hart's S-mode
        // for the uart and virtio disk.
        let senable_ref = PLIC_SENABLE!(hart) as *mut u32;
        senable_ref.write_volatile((1 << UART0_IRQ) | (1 << VIRTIO0_IRQ));

        // set this hart's S-mode priority threshold to 0.
        let spriority_ref = PLIC_SPRIORITY!(hart) as *mut u32;
        spriority_ref.write_volatile(0);
    }
}

// ask the PLIC what interrupt we should serve.
pub(crate) fn plic_claim() -> u32 {
    let hart = cpuid();
    let irq = PLIC_SCLAIM!(hart) as *const u32;
    unsafe { irq.read_volatile() }
}

// tell the PLIC we've served this IRQ.
pub(crate) fn plic_complete(irq: u32) {
    let hart = cpuid();
    unsafe {
        (PLIC_SCLAIM!(hart) as *mut u32).write_volatile(irq);
    }
}
