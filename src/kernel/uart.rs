use crate::spinlock::{pop_off, push_off, Spinlock};

// the UART control registers are memory-mapped
// at address UART0. this macro returns the
// address of one of the registers.
#[macro_export]
macro_rules! Reg {
    ( $reg:expr ) => {
        $crate::memlayout::UART0 + ($reg as usize)
    };
}

// the UART control registers.
// some have different meanings for
// read vs write.
// see http://byterunner.com/16550.html
pub const RHR: u8 = 0;                 // receive holding register (for input bytes)
pub const THR: u8 = 0;                 // transmit holding register (for output bytes)
pub const IER: u8 = 1;                 // interrupt enable register
pub const IER_RX_ENABLE: u8 = 1<<0;
pub const IER_TX_ENABLE: u8 = 1<<1;
pub const FCR: u8 = 2;                 // FIFO control register
pub const FCR_FIFO_ENABLE: u8 = 1<<0;
pub const FCR_FIFO_CLEAR: u8 = 3<<1;   // clear the content of the two FIFOs
pub const ISR: u8 = 2;                 // interrupt status register
pub const LCR: u8 = 3;                 // line control register
pub const LCR_EIGHT_BITS: u8 = 3<<0;
pub const LCR_BAUD_LATCH: u8 = 1<<7;   // special mode to set baud rate
pub const LSR: u8 = 5;                 // line status register
pub const LSR_RX_READY: u8 = 1<<0;     // input is waiting to be read from RHR
pub const LSR_TX_IDLE: u8 = 1<<5;      // THR can accept another character to send
pub const UART_TX_BUF_SIZE: usize = 32;

#[macro_export]
macro_rules! ReadReg {
    ( $reg:expr ) => {
        unsafe {
            ($crate::Reg!($reg) as *mut u8).read_volatile()
        }
    };
}

#[macro_export]
macro_rules! WriteReg {
    ( $reg:expr, $val:expr ) => {
        unsafe {
            ($crate::Reg!($reg) as *mut u8).write_volatile($val)
        }
    };
}

pub struct Uart {
    uart_tx_lock: Spinlock,
    uart_tx_buf: [u8; UART_TX_BUF_SIZE],
    uart_tx_w: usize,
    uart_tx_r: usize,
}

impl Uart {
    pub const fn create() -> Self {
        Self {
            uart_tx_lock: Spinlock::init_lock("uart"),
            uart_tx_buf: [0; UART_TX_BUF_SIZE],
            uart_tx_w: 0,
            uart_tx_r: 0,
        }
    }

    pub fn init() {
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
    }

    /// add a character to the output buffer and tell the
    /// UART to start sending if it isn't already.
    /// blocks if the output buffer is full.
    /// because it may block, it can't be called
    /// from interrupts; it's only suitable for use
    /// by write().
    pub fn putc(self: &mut Self, c: u8) {
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
        self.start();
        self.uart_tx_lock.release();
    }

    /// alternate version of uartputc() that doesn't
    /// use interrupts, for use by kernel printf() and
    /// to echo characters. it spins waiting for the uart's
    /// output register to be empty.
    pub fn putc_sync(self: &mut Self, c: u8) {
        push_off();

        // TODO: panicked logic
        // if(panicked){
        // for(;;)
        //     ;
        // }

        // wait for Transmit Holding Empty to be set in LSR.
        while (ReadReg!(LSR) & LSR_TX_IDLE) == 0 {}
        WriteReg!(THR, c);

        pop_off();
    }

    /// if the UART is idle, and a character is waiting
    /// in the transmit buffer, send it.
    /// caller must hold uart_tx_lock.
    /// called from both the top- and bottom-half.
    fn start(self: &mut Self) {
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
    fn getc(self: &Self) -> i8 {
        return if ReadReg!(LSR) & 0x01 != 0 {
            // input data is ready.
            ReadReg!(RHR) as i8
        } else {
            -1
        }
    }

    /// handle a uart interrupt, raised because input has
    /// arrived, or the uart is ready for more output, or
    /// both. called from devintr().
    fn intr(self: &mut Self) {
        // read and process incoming characters.
        loop {
            let c = self.getc();
            if c == -1 {
                break;
            }
            // TODO: connect with console
            // consoleintr(c);
        }

        // send buffered characters.
        self.uart_tx_lock.acquire();
        self.start();
        self.uart_tx_lock.release();
    }
}