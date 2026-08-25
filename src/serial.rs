//! UART (SCI2 = Arduino Serial1, pins D0/D1 = P301/P302).
//!
//! Provides asynchronous 8N1 send/receive. Implements embedded-io, embedded-hal-nb, and
//! core::fmt::Write, so `write!`/`writeln!` work directly.
//!
//! # Baud rate
//!
//! The RA4M1 SCI's asynchronous baud rate follows (with SEMR.ABCS=1, BGDM=1):
//!
//! ```text
//! BRR = PCLKB / (divisor * baud) - 1,  divisor = 16 * 2^(2n-1) = 8 << (2n)
//! ```
//!
//! `n` (= SMR.CKS, 0..3) is chosen as the smallest value for which BRR fits in 0..255.

use crate::clock::Clocks;
use crate::gpio::{Alternate, Pin};
use ra4m1::SCI2;

/// PSEL value that assigns SCI to the asynchronous serial function (RA4M1 I/O port SCI selection = 0b00100).
const PSEL_SCI: u8 = 0b0_0100;

/// Receive error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Overrun (receive buffer overwritten).
    Overrun,
    /// Framing error (invalid stop bit).
    Framing,
    /// Parity error.
    Parity,
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        embedded_io::ErrorKind::Other
    }
}

impl embedded_hal_nb::serial::Error for Error {
    fn kind(&self) -> embedded_hal_nb::serial::ErrorKind {
        match self {
            Error::Overrun => embedded_hal_nb::serial::ErrorKind::Overrun,
            Error::Framing => embedded_hal_nb::serial::ErrorKind::FrameFormat,
            Error::Parity => embedded_hal_nb::serial::ErrorKind::Parity,
        }
    }
}

/// An invalid or insufficiently accurate baud-rate request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidBaudRate;

/// UART backed by SCI2.
pub struct Serial {
    sci: SCI2,
    _tx: Pin<'3', 2, Alternate>,
    _rx: Pin<'3', 1, Alternate>,
}

/// Finds the smallest `n` for which BRR fits in 0..=255, given `divisor = 8 << (2n)`
/// (ABCS=1, BGDM=1), and returns `(BRR, CKS)`.
const fn calc_brr(pclka: u32, baud: u32) -> Option<(u8, u8)> {
    if baud == 0 {
        return None;
    }

    let mut n = 0u32;
    while n <= 3 {
        let div = 8u64 << (2 * n);
        let denom = div * baud as u64;
        // Round (N+1) to the nearest integer.
        let val = (pclka as u64 + denom / 2) / denom;
        if val >= 1 && val <= 256 {
            // Reject settings whose baud-rate error exceeds 3%. Large errors can otherwise
            // look representable near the upper end of the baud-rate range.
            let target = denom * val;
            let error = (pclka as u64).abs_diff(target);
            if error * 100 <= target * 3 {
                return Some(((val - 1) as u8, n as u8));
            }
            return None;
        }
        n += 1;
    }
    None
}

/// Releases the SCI2 module stop (MSTPCRB.MSTPB29=0).
fn enable_sci2() {
    critical_section::with(|_| unsafe {
        let system = &*ra4m1::SYSTEM::PTR;
        let mstp = &*ra4m1::MSTP::PTR;
        // Unlock write protection for low-power/module-stop registers (PRC1).
        system.prcr.write(|w| w.bits(0xA502));
        mstp.mstpcrb.modify(|_, w| w.mstpb29()._0());
        system.prcr.write(|w| w.bits(0xA500));
    });
}

impl Serial {
    /// Consumes SCI2 and the TX(D1=P302)/RX(D0=P301) pins, and builds a UART at `baud` bps.
    pub fn new<M1, M2>(
        sci: SCI2,
        tx: Pin<'3', 2, M1>,
        rx: Pin<'3', 1, M2>,
        baud: u32,
        clocks: &Clocks,
    ) -> Result<Self, InvalidBaudRate> {
        let (brr, cks) = calc_brr(clocks.pclka().to_Hz(), baud).ok_or(InvalidBaudRate)?;
        enable_sci2();

        // Stop TX/RX while configuring.
        sci.scr().write(|w| unsafe { w.bits(0) });

        // Restore all format-related registers that a bootloader may have changed.
        sci.smr().reset();
        sci.scmr.reset();
        sci.semr.reset();
        sci.snfr.reset();

        // Assign the pins to the SCI2 function.
        let _tx = tx.into_alternate(PSEL_SCI);
        let _rx = rx.into_alternate(PSEL_SCI);

        // SMR: asynchronous, 8-bit, no parity, 1 stop bit, CKS=cks.
        sci.smr().write(|w| unsafe { w.bits(cks) });
        // SEMR: ABCS=1, BGDM=1 (improves baud rate resolution).
        sci.semr.write(|w| w.abcs()._1().bgdm()._1());
        sci.brr.write(|w| unsafe { w.bits(brr) });
        sci.ssr().modify(|_, w| w.orer()._0().fer()._0().per()._0());

        // The manual requires waiting at least one bit time after setting BRR before
        // enabling TX/RX.
        cortex_m::asm::delay(clocks.sysclk().to_Hz() / baud + 64);

        // Enable TX/RX (polling only, no interrupts).
        sci.scr().modify(|_, w| w.te()._1().re()._1());

        Ok(Self { sci, _tx, _rx })
    }

    /// Releases the SCI2 handle and pins.
    pub fn release(self) -> (SCI2, Pin<'3', 2, Alternate>, Pin<'3', 1, Alternate>) {
        self.sci.scr().write(|w| unsafe { w.bits(0) });
        (self.sci, self._tx, self._rx)
    }

    /// Sends one byte (waits for the transmit data register to be empty).
    pub fn write_byte(&mut self, byte: u8) -> nb::Result<(), core::convert::Infallible> {
        if self.sci.ssr().read().tdre().is_0() {
            return Err(nb::Error::WouldBlock);
        }
        self.sci.tdr.write(|w| unsafe { w.bits(byte) });
        Ok(())
    }

    /// Waits for the transmission to complete (TEND).
    pub fn flush(&mut self) -> nb::Result<(), core::convert::Infallible> {
        if self.sci.ssr().read().tend().is_0() {
            return Err(nb::Error::WouldBlock);
        }
        Ok(())
    }

    /// Receives one byte (waits for reception to complete). Clears error flags before
    /// returning them.
    pub fn read_byte(&mut self) -> nb::Result<u8, Error> {
        let ssr = self.sci.ssr().read();
        if ssr.orer().is_1() || ssr.fer().is_1() || ssr.per().is_1() {
            let err = if ssr.orer().is_1() {
                Error::Overrun
            } else if ssr.fer().is_1() {
                Error::Framing
            } else {
                Error::Parity
            };
            self.sci
                .ssr()
                .modify(|_, w| w.orer()._0().fer()._0().per()._0());
            return Err(nb::Error::Other(err));
        }
        if self.sci.ssr().read().rdrf().is_0() {
            return Err(nb::Error::WouldBlock);
        }
        Ok(self.sci.rdr.read().bits())
    }
}

// ===== embedded-hal-nb =====

impl embedded_hal_nb::serial::ErrorType for Serial {
    type Error = Error;
}

impl embedded_hal_nb::serial::Write<u8> for Serial {
    fn write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        // write_byte's Infallible has no error variant to map on the TX side.
        self.write_byte(word).map_err(|e| match e {
            nb::Error::WouldBlock => nb::Error::WouldBlock,
            nb::Error::Other(_) => unreachable!(),
        })
    }
    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        Serial::flush(self).map_err(|e| match e {
            nb::Error::WouldBlock => nb::Error::WouldBlock,
            nb::Error::Other(_) => unreachable!(),
        })
    }
}

impl embedded_hal_nb::serial::Read<u8> for Serial {
    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        self.read_byte()
    }
}

// ===== embedded-io =====

impl embedded_io::ErrorType for Serial {
    type Error = Error;
}

impl embedded_io::Write for Serial {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        for &b in buf {
            nb::block!(self.write_byte(b)).ok();
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        nb::block!(Serial::flush(self)).ok();
        Ok(())
    }
}

impl embedded_io::Read for Serial {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }
        // Block for at least one byte.
        buf[0] = nb::block!(self.read_byte())?;
        Ok(1)
    }
}

// ===== core::fmt::Write (for write!/writeln!) =====

impl core::fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            nb::block!(self.write_byte(b)).ok();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::calc_brr;

    #[test]
    fn calculates_common_baud_rates_at_48_mhz() {
        assert_eq!(calc_brr(48_000_000, 115_200), Some((51, 0)));
        assert_eq!(calc_brr(48_000_000, 9_600), Some((155, 1)));
        assert_eq!(calc_brr(48_000_000, 1_200), Some((77, 3)));
        assert_eq!(calc_brr(48_000_000, 600), Some((155, 3)));
    }

    #[test]
    fn accepts_exact_maximum_baud_rate() {
        assert_eq!(calc_brr(48_000_000, 6_000_000), Some((0, 0)));
    }

    #[test]
    fn rejects_zero_out_of_range_and_inaccurate_rates() {
        assert_eq!(calc_brr(48_000_000, 0), None);
        assert_eq!(calc_brr(48_000_000, 300), None);
        assert_eq!(calc_brr(48_000_000, 8_000_000), None);
        assert_eq!(calc_brr(48_000_000, 4_000_000), None);
    }
}
