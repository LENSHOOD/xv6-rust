//
// the riscv Platform Level Interrupt Controller (PLIC).
//

use crate::memlayout::{PLIC, UART0_IRQ, VIRTIO0_IRQ};
use crate::{PLIC_SENABLE, PLIC_SPRIORITY, proc};

pub fn plicinit() {
    unsafe {
        // set desired IRQ priorities non-zero (otherwise disabled).
        let uart_irq_ref = ((PLIC + UART0_IRQ*4) as * mut u32).as_mut().unwrap();
        *(uart_irq_ref) = 1;
        let virtio_irq_ref = ((PLIC + VIRTIO0_IRQ*4) as * mut u32).as_mut().unwrap();
        *(virtio_irq_ref) = 1;
    }
}

pub fn plicinithart() {
    let hart = proc::cpuid();

    unsafe {
        // set enable bits for this hart's S-mode
        // for the uart and virtio disk.
        let senable_ref = (PLIC_SENABLE!(hart) as * mut u32).as_mut().unwrap();
        *(senable_ref) = (1 << UART0_IRQ) | (1 << VIRTIO0_IRQ);

        // set this hart's S-mode priority threshold to 0.
        let spriority_ref = (PLIC_SPRIORITY!(hart) as * mut u32).as_mut().unwrap();
        *(spriority_ref) = 0;
    }
}
