//! A/D converter (ADC140, 14-bit successive approximation).
//!
//! Corresponds to the Arduino analog inputs A0-A5.
//!
//! | Arduino | RA4M1 pin | ADC channel |
//! |---------|-----------|-------------|
//! | A0      | P014      | AN009       |
//! | A1      | P000      | AN000       |
//! | A2      | P001      | AN001       |
//! | A3      | P002      | AN002       |
//! | A4      | P101      | AN021       |
//! | A5      | P100      | AN022       |
//!
//! Note: A4/A5 are shared with I2C (SDA/SCL).
//!
//! ```ignore
//! let mut adc = Adc::new(p.adc140, &clocks);
//! let a0 = p.pins.a0.into_analog();
//! let value: u16 = adc.read(&a0); // 0..=16383 (14-bit)
//! ```

use crate::clock::Clocks;
use crate::gpio::{Analog, Pin};
use ra4m1::ADC140;

/// Maps an analog pin to its ADC channel number.
///
/// Only implemented for valid analog pins (A0-A5), so passing an unsupported pin to
/// [`Adc::read`] is a compile error.
pub trait AnalogChannel {
    /// ADC channel number (AN) for this pin.
    const CHANNEL: u8;
}

impl AnalogChannel for Pin<'0', 14, Analog> {
    const CHANNEL: u8 = 9; // A0 = P014 = AN009
}
impl AnalogChannel for Pin<'0', 0, Analog> {
    const CHANNEL: u8 = 0; // A1 = P000 = AN000
}
impl AnalogChannel for Pin<'0', 1, Analog> {
    const CHANNEL: u8 = 1; // A2 = P001 = AN001
}
impl AnalogChannel for Pin<'0', 2, Analog> {
    const CHANNEL: u8 = 2; // A3 = P002 = AN002
}
impl AnalogChannel for Pin<'1', 1, Analog> {
    const CHANNEL: u8 = 21; // A4 = P101 = AN021
}
impl AnalogChannel for Pin<'1', 0, Analog> {
    const CHANNEL: u8 = 22; // A5 = P100 = AN022
}

/// 14-bit A/D converter backed by ADC140.
pub struct Adc {
    adc: ADC140,
}

/// Releases the ADC140 module stop (MSTPCRD.MSTPD16=0).
fn enable_adc140() {
    critical_section::with(|_| unsafe {
        let system = &*ra4m1::SYSTEM::PTR;
        let mstp = &*ra4m1::MSTP::PTR;
        system.prcr.write(|w| w.bits(0xA502));
        mstp.mstpcrd.modify(|_, w| w.mstpd16()._0());
        system.prcr.write(|w| w.bits(0xA500));
    });
}

impl Adc {
    /// Consumes ADC140 and initializes it in 14-bit, right-aligned, single-scan mode.
    pub fn new(adc: ADC140, _clocks: &Clocks) -> Self {
        enable_adc140();
        // 14-bit resolution (default is 12-bit), data right-aligned.
        adc.adcer.modify(|_, w| w.adprc()._11().adrfmt()._0());
        // Single-scan mode.
        adc.adcsr.modify(|_, w| w.adcs()._00());
        Self { adc }
    }

    /// Converts the given analog pin once and returns the 14-bit result (0..=16383).
    pub fn read<P: AnalogChannel>(&mut self, _pin: &P) -> u16 {
        let ch = P::CHANNEL;
        // Select only the target channel (ch<16 -> ADANSA0, ch>=16 -> ADANSA1).
        if ch < 16 {
            self.adc.adansa0.write(|w| unsafe { w.bits(1u16 << ch) });
            self.adc.adansa1.write(|w| unsafe { w.bits(0) });
        } else {
            self.adc.adansa0.write(|w| unsafe { w.bits(0) });
            self.adc
                .adansa1
                .write(|w| unsafe { w.bits(1u16 << (ch - 16)) });
        }
        // Start conversion, wait for completion (ADST=0).
        self.adc.adcsr.modify(|_, w| w.adst()._1());
        while self.adc.adcsr.read().adst().bit_is_set() {}
        // Read the data register (ch0..14 use an array, ch21/22 have dedicated registers).
        match ch {
            21 => self.adc.addr21.read().bits(),
            22 => self.adc.addr22.read().bits(),
            _ => self.adc.addr[ch as usize].read().bits(),
        }
    }

    /// Releases the ADC140 handle.
    pub fn release(self) -> ADC140 {
        self.adc
    }
}
