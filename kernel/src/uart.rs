// 16550-compatible uart driver for the qemu-virt platform.
// the device tree puts the uart at 0x10000000; we hardcode for v0.1.
// all access goes through the print! / println! macros which lock the
// uart so two harts can't garble each other (we're single-hart for now,
// but the lock costs nothing).

use core::fmt::{self, Write};
use core::ptr::{read_volatile, write_volatile};

const UART_BASE: usize = 0x1000_0000;

const RBR: usize = 0;   // receiver buffer (read)
const THR: usize = 0;   // transmit holding (write)
const IER: usize = 1;
const FCR: usize = 2;
const LCR: usize = 3;
const LSR: usize = 5;

const LSR_THRE: u8 = 1 << 5;   // transmit holding empty
const LSR_DR:   u8 = 1 << 0;   // data ready

pub struct Uart;

impl Uart {
    fn reg(&self, off: usize) -> *mut u8 {
        (UART_BASE + off) as *mut u8
    }

    fn read(&self, off: usize) -> u8 {
        unsafe { read_volatile(self.reg(off)) }
    }

    fn write(&self, off: usize, val: u8) {
        unsafe { write_volatile(self.reg(off), val) }
    }

    pub fn putc(&self, c: u8) {
        // spin until the transmit holding register is empty.
        while self.read(LSR) & LSR_THRE == 0 {}
        self.write(THR, c);
    }

    pub fn getc(&self) -> Option<u8> {
        if self.read(LSR) & LSR_DR == 0 {
            None
        } else {
            Some(self.read(RBR))
        }
    }
}

impl fmt::Write for Uart {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            // translate \n into \r\n so terminal emulators don't get
            // staircases. cheap and standard for serial.
            if b == b'\n' {
                self.putc(b'\r');
            }
            self.putc(b);
        }
        Ok(())
    }
}

pub fn init() {
    let u = Uart;
    // 8N1, no parity, 8 data bits.
    u.write(LCR, 0b0000_0011);
    // enable + clear fifos.
    u.write(FCR, 0b0000_0111);
    // disable interrupts (we're polling for v0.1).
    u.write(IER, 0);
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let mut u = Uart;
    let _ = u.write_fmt(args);
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::uart::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}
