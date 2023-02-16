use core::fmt::{Error, Write};
use crate::spinlock::{pop_off, push_off, Spinlock};

// the UART control registers are memory-mapped
// at address UART0. this macro returns the
// address of one of the registers.
#[macro_export]
macro_rules! Reg {
    ( $reg:expr ) => {
        $crate::memlayout::UART0 + $reg
    };
}

// the UART control registers.
// some have different meanings for
// read vs write.
// see http://byterunner.com/16550.html
pub const RHR: u64 = 0;                 // receive holding register (for input bytes)
pub const THR: u64 = 0;                 // transmit holding register (for output bytes)
pub const IER: u64 = 1;                 // interrupt enable register
pub const IER_RX_ENABLE: u64 = 1<<0;
pub const IER_TX_ENABLE: u64 = 1<<1;
pub const FCR: u64 = 2;                 // FIFO control register
pub const FCR_FIFO_ENABLE: u64 = 1<<0;
pub const FCR_FIFO_CLEAR: u64 = 3<<1; // clear the content of the two FIFOs
pub const ISR: u64 = 2;                 // interrupt status register
pub const LCR: u64 = 3;                 // line control register
pub const LCR_EIGHT_BITS: u64 = 3<<0;
pub const LCR_BAUD_LATCH: u64 = 1<<7; // special mode to set baud rate
pub const LSR: u64 = 5;                 // line status register
pub const LSR_RX_READY: u64 = 1<<0;   // input is waiting to be read from RHR
pub const LSR_TX_IDLE: u64 = 1<<5;    // THR can accept another character to send
pub const UART_TX_BUF_SIZE: usize = 32;

#[macro_export]
macro_rules! ReadReg {
    ( $reg:expr ) => {
        ($crate::uart::Reg!($reg) as *mut u8).read_volatile()
    };
}

#[macro_export]
macro_rules! WriteReg {
    ( $reg:expr, $val:expr ) => {
        ($crate::uart::Reg!($reg) as *mut u8).write_volatile($val)
    };
}

struct Uart {
    uart_tx_lock: Spinlock,
    uart_tx_buf: [u8; UART_TX_BUF_SIZE],
    uart_tx_w: u64,
    uart_tx_r: u64,
}

impl Uart {
    pub fn uart_init() -> Self {
        // disable interrupts.
        WriteReg!(IER, 0x00);

        // special mode to set baud rate.
        WriteReg!(LCR, LCR_BAUD_LATCH);

        // LSB for baud rate of 38.4K.
        WriteReg!(0, 0x03);

        // MSB for baud rate of 38.4K.
        WriteReg!(1, 0x00);

        // leave set-baud mode,
        // and set word length to 8 bits, no parity.
        WriteReg!(LCR, LCR_EIGHT_BITS);

        // reset and enable FIFOs.
        WriteReg!(FCR, FCR_FIFO_ENABLE | FCR_FIFO_CLEAR);

        // enable transmit and receive interrupts.
        WriteReg!(IER, IER_TX_ENABLE | IER_RX_ENABLE);

        Self {
            uart_tx_lock: Spinlock::init_lock("uart"),
            uart_tx_buf: [0; UART_TX_BUF_SIZE],
        }
    }

    /// add a character to the output buffer and tell the
    /// UART to start sending if it isn't already.
    /// blocks if the output buffer is full.
    /// because it may block, it can't be called
    /// from interrupts; it's only suitable for use
    /// by write().
    fn uart_putc(self: &mut Self, c: u8) {
        self.uart_tx_lock.acquire();

        // TODO: panicked logic
        // if(panicked){
        //     for(;;)
        //     ;
        // }

        while self.uart_tx_w == self.uart_tx_r + UART_TX_BUF_SIZE {
            // buffer is full.
            // wait for uartstart() to open up space in the buffer.
            // TODO: no sched yet
            // sleep(&uart_tx_r, &uart_tx_lock);
        }
        self.uart_tx_buf[self.uart_tx_w % UART_TX_BUF_SIZE] = c;
        self.uart_tx_w += 1;
        self.uart_start();
        self.uart_tx_lock.release();
    }

    /// alternate version of uartputc() that doesn't
    /// use interrupts, for use by kernel printf() and
    /// to echo characters. it spins waiting for the uart's
    /// output register to be empty.
    fn uart_putc_sync(c: u8) {
        push_off();

        // TODO: panicked logic
        // if(panicked){
        // for(;;)
        //     ;
        // }

        // wait for Transmit Holding Empty to be set in LSR.
        while (ReadReg!(LSR) & LSR_TX_IDLE) == 0 {
            ;
        }
        WriteReg!(THR, c);

        pop_off();
    }

    /// if the UART is idle, and a character is waiting
    /// in the transmit buffer, send it.
    /// caller must hold uart_tx_lock.
    /// called from both the top- and bottom-half.
    fn uart_start(self: &mut Self) {
        loop {
            if self.uart_tx_w == self.uart_tx_r {
                // transmit buffer is empty.
                return;
            }

            if (ReadReg!(LSR) & LSR_TX_IDLE) == 0 {
                // the UART transmit holding register is full,
                // so we cannot give it another byte.
                // it will interrupt when it's ready for a new byte.
                return;
            }

            let c = self.uart_tx_buf[self.uart_tx_r % UART_TX_BUF_SIZE];
            self.uart_tx_r += 1;

            // maybe uartputc() is waiting for space in the buffer.
            // TODO: no sched yet
            // wakeup(&uart_tx_r);

            WriteReg!(THR, c);
        }
    }

    /// read one input character from the UART.
    /// return -1 if none is waiting.
    fn uart_getc(self: &Self) -> u8 {
        if ReadReg!(LSR) & 0x01 {
            // input data is ready.
            return ReadReg!(RHR);
        } else {
            return -1;
        }
    }

    /// handle a uart interrupt, raised because input has
    /// arrived, or the uart is ready for more output, or
    /// both. called from devintr().
    fn uart_intr(self: &Self) {
        // read and process incoming characters.
        loop {
            let c = self.uart_getc();
            if c == -1 {
                break;
            }
            consoleintr(c);
        }

        // send buffered characters.
        self.uart_tx_lock.acquire();
        self.uartstart();
        self.uart_tx_lock.release();
    }

}

pub struct UartDriver {
    base_address: usize,
}

impl UartDriver {
    pub fn new(base_address: usize) -> Self {
        UartDriver {
            // Since our parameter is also named the same as the member
            // variable, we can just label it by name.
            base_address
        }
    }

    pub fn init(&self) {
        let ptr = self.base_address as *mut u8;
        unsafe {
            // First, set the word length, which
            // are bits 0, and 1 of the line control register (LCR)
            // which is at base_address + 3
            // We can easily write the value 3 here or 0b11, but I'm
            // extending it so that it is clear we're setting two individual
            // fields
            //         Word 0     Word 1
            //         ~~~~~~     ~~~~~~
            let lcr = (1 << 0) | (1 << 1);
            ptr.add(3).write_volatile(lcr);

            // Now, enable the FIFO, which is bit index 0 of the FIFO
            // control register (FCR at offset 2).
            // Again, we can just write 1 here, but when we use left shift,
            // it's easier to see that we're trying to write bit index #0.
            ptr.add(2).write_volatile(1 << 0);

            // Enable receiver buffer interrupts, which is at bit index
            // 0 of the interrupt enable register (IER at offset 1).
            ptr.add(1).write_volatile(1 << 0);

            // If we cared about the divisor, the code below would set the divisor
            // from a global clock rate of 22.729 MHz (22,729,000 cycles per second)
            // to a signaling rate of 2400 (BAUD). We usually have much faster signalling
            // rates nowadays, but this demonstrates what the divisor actually does.
            // The formula given in the NS16500A specification for calculating the divisor
            // is:
            // divisor = ceil( (clock_hz) / (baud_sps x 16) )
            // So, we substitute our values and get:
            // divisor = ceil( 22_729_000 / (2400 x 16) )
            // divisor = ceil( 22_729_000 / 38_400 )
            // divisor = ceil( 591.901 ) = 592

            // The divisor register is two bytes (16 bits), so we need to split the value
            // 592 into two bytes. Typically, we would calculate this based on measuring
            // the clock rate, but again, for our purposes [qemu], this doesn't really do
            // anything.
            let divisor: u16 = 592;
            let divisor_least: u8 = (divisor & 0xff) as u8;
            let divisor_most:  u8 = (divisor >> 8) as u8;

            // Notice that the divisor register DLL (divisor latch least) and DLM (divisor
            // latch most) have the same base address as the receiver/transmitter and the
            // interrupt enable register. To change what the base address points to, we
            // open the "divisor latch" by writing 1 into the Divisor Latch Access Bit
            // (DLAB), which is bit index 7 of the Line Control Register (LCR) which
            // is at base_address + 3.
            ptr.add(3).write_volatile(lcr | 1 << 7);

            // Now, base addresses 0 and 1 point to DLL and DLM, respectively.
            // Put the lower 8 bits of the divisor into DLL
            ptr.add(0).write_volatile(divisor_least);
            ptr.add(1).write_volatile(divisor_most);

            // Now that we've written the divisor, we never have to touch this again. In
            // hardware, this will divide the global clock (22.729 MHz) into one suitable
            // for 2,400 signals per second. So, to once again get access to the
            // RBR/THR/IER registers, we need to close the DLAB bit by clearing it to 0.
            ptr.add(3).write_volatile(lcr);
        }
    }

    fn get(&self) -> Option<u8> {
        let ptr = self.base_address as *mut u8;
        unsafe {
            // Bit index #5 is the Line Control Register.
            if ptr.add(5).read_volatile() & 1 == 0 {
                // The DR bit is 0, meaning no data
                None
            }
            else {
                // The DR bit is 1, meaning data!
                Some(ptr.add(0).read_volatile())
            }
        }
    }

    fn put(&self, c: u8) {
        let ptr = self.base_address as *mut u8;
        unsafe {
            // If we get here, the transmitter is empty, so transmit
            // our stuff!
            ptr.add(0).write_volatile(c);
        }
    }
}

impl Write for UartDriver {
    // The trait Write expects us to write the function write_str
    // which looks like:
    fn write_str(&mut self, s: &str) -> Result<(), Error> {
        for c in s.bytes() {
            self.put(c);
        }
        // Return that we succeeded.
        Ok(())
    }
}