//! PWM output (GPT timer, sawtooth PWM mode).
//!
//! Corresponds to the Arduino PWM pins (marked `~`). Each pin is wired to a fixed GPT
//! channel/output. The mapping is selected by the `uno-r4-minima` (default) or
//! `uno-r4-wifi` Cargo feature.
//!
//! UNO R4 Minima:
//!
//! | Arduino | RA4M1 | GPT output | GPT channel |
//! |---------|-------|-----------|-------------|
//! | D6      | P106  | GTIOC0B   | GPT320 (ch0) |
//! | D3      | P104  | GTIOC1B   | GPT321 (ch1) |
//! | D11     | P109  | GTIOC1A   | GPT321 (ch1) |
//! | D5      | P102  | GTIOC2B   | GPT162 (ch2) |
//! | D10     | P112  | GTIOC3B   | GPT163 (ch3) |
//! | D9      | P303  | GTIOC7B   | GPT167 (ch7) |
//!
//! UNO R4 WiFi:
//!
//! | Arduino | RA4M1 | GPT output | GPT channel |
//! |---------|-------|------------|-------------|
//! | D3      | P105  | GTIOC1A    | GPT321 (ch1) |
//! | D5      | P107  | GTIOC0A    | GPT320 (ch0) |
//! | D6      | P111  | GTIOC3A    | GPT163 (ch3) |
//! | D9      | P303  | GTIOC7B    | GPT167 (ch7) |
//! | D10     | P103  | GTIOC2A    | GPT162 (ch2) |
//! | D11     | P411  | GTIOC6A    | GPT166 (ch6) |
//!
//! On the Minima, D3 and D11 share GPT321, so only one can be used at a time
//! (the type system enforces this too, since `gpt.gpt321` can only be consumed once).
//!
//! Implements [`embedded_hal::pwm::SetDutyCycle`]. Frequency is fixed at construction time.
//!
//! ```ignore
//! use embedded_hal::pwm::SetDutyCycle;
//! let mut led = PwmD6::new(p.gpt.gpt320, p.pins.d6, 1_000, &clocks).unwrap();
//! led.set_duty_cycle_percent(25).unwrap();
//! ```

use crate::clock::Clocks;
use crate::gpio::{Alternate, Pin};

/// PSEL value that assigns a pin's GTIOC to PWM (RA4M1 I/O port GPT(GTIOC) selection = 0b00011).
const PSEL_GPT: u8 = 0b0_0011;

/// PAC handles for the GPT channels used by PWM. Provided by [`crate::Peripherals::take`].
#[allow(missing_docs)]
pub struct GptChannels {
    pub gpt320: ra4m1::GPT320, // 32-bit ch0
    pub gpt321: ra4m1::GPT321, // 32-bit ch1
    pub gpt162: ra4m1::GPT162, // 16-bit ch2
    pub gpt163: ra4m1::GPT163, // 16-bit ch3
    pub gpt166: ra4m1::GPT166, // 16-bit ch6
    pub gpt167: ra4m1::GPT167, // 16-bit ch7
}

/// Releases the module stop for the given GPT channel group.
/// GPT320/GPT321 use MSTPCRD.MSTPD5; GPT162..GPT167 use MSTPD6.
fn enable_gpt(is_32_bit: bool) {
    critical_section::with(|_| unsafe {
        let system = &*ra4m1::SYSTEM::PTR;
        let mstp = &*ra4m1::MSTP::PTR;
        system.prcr.write(|w| w.bits(0xA502));
        if is_32_bit {
            mstp.mstpcrd.modify(|_, w| w.mstpd5()._0());
        } else {
            mstp.mstpcrd.modify(|_, w| w.mstpd6()._0());
        }
        system.prcr.write(|w| w.bits(0xA500));
    });
}

/// A zero, out-of-range, or insufficiently accurate PWM frequency.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFrequency;

/// Finds the (prescaler, GTPR) pair that most closely realizes `freq` Hz.
///
/// `GTPR+1 = PCLKD / (prescaler * freq)`. Picks the smallest prescaler for which GTPR+1
/// fits in a u16 (the embedded-hal duty cycle limit). Configurations with more than 3%
/// frequency error are rejected.
fn compute_period(pclkd: u32, freq: u32) -> Option<(u8, u32)> {
    if freq == 0 {
        return None;
    }

    // (TPCS value, divisor)
    const DIVS: [(u8, u32); 6] = [(0, 1), (1, 4), (2, 16), (3, 64), (4, 256), (5, 1024)];
    let mut i = 0;
    while i < DIVS.len() {
        let (tpcs, div) = DIVS[i];
        let timer_hz = pclkd / div;
        let ticks = (u64::from(timer_hz) + u64::from(freq) / 2) / u64::from(freq);
        if (2..=u16::MAX as u64).contains(&ticks) {
            let target = u64::from(freq) * ticks;
            let error = u64::from(timer_hz).abs_diff(target);
            if error * 100 <= target * 3 {
                return Some((tpcs, ticks as u32 - 1));
            }
        }
        i += 1;
    }
    None
}

/// GTIOR value: output B as sawtooth PWM (starts low, goes high at period end, back to low
/// on GTCCRB match) plus output enable. GTIOB[20:16]=0b01001(=9), OBE(bit24)=1.
const GTIOR_OUTPUT_B: u32 = (0b0_1001 << 16) | (1 << 24);
/// GTIOR value for output A. GTIOA[4:0]=0b01001, OAE(bit8)=1.
const GTIOR_OUTPUT_A: u32 = 0b0_1001 | (1 << 8);

macro_rules! pwm_channel {
    (
        $(#[$meta:meta])*
        $ctor:ident, $name:ident, $ch_ty:ident, $port:literal, $pin_num:literal,
        $is_32_bit:expr, $ccr:ident, $buffer:ident, $duty_shift:literal, $gtior:expr
    ) => {
        $(#[$meta])*
        pub struct $name {
            gpt: ra4m1::$ch_ty,
            _pin: Pin<$port, $pin_num, Alternate>,
            max_duty: u16,
        }

        impl $name {
            /// Consumes the GPT channel and pin, and starts a `freq` Hz PWM at 0% duty.
            pub fn new<M>(
                gpt: ra4m1::$ch_ty,
                pin: Pin<$port, $pin_num, M>,
                freq: u32,
                clocks: &Clocks,
            ) -> Result<Self, InvalidFrequency> {
                let (tpcs, gtpr) =
                    compute_period(clocks.pclkd().to_Hz(), freq).ok_or(InvalidFrequency)?;
                enable_gpt($is_32_bit);
                unsafe {
                    gpt.gtwp.write(|w| w.bits(0x0000_A500)); // unlock write protection
                    gpt.gtcr.write(|w| w.bits((tpcs as u32) << 24)); // MD=sawtooth(0), CST=stopped, prescaler
                    gpt.gtst.write(|w| w.bits(0));
                    // Disable any event/interrupt configuration left by a bootloader.
                    gpt.gtssr.write(|w| w.bits(0x8000_0000));
                    gpt.gtpsr.write(|w| w.bits(0x8000_0000));
                    gpt.gtcsr.write(|w| w.bits(0x8000_0000));
                    gpt.gtupsr.write(|w| w.bits(0));
                    gpt.gtdnsr.write(|w| w.bits(0));
                    gpt.gtintad.write(|w| w.bits(0));
                    gpt.gtdtcr.write(|w| w.bits(0));
                    gpt.gtpbr.write(|w| w.bits(gtpr));
                    gpt.gtpr.write(|w| w.bits(gtpr));
                    gpt.$ccr.write(|w| w.bits(0));
                    gpt.$buffer.write(|w| w.bits(0));
                    // Single-buffer GTCCRA/GTCCRB and GTPR, then force the initial transfer.
                    gpt.gtber.write(|w| w.bits(0x0055_0000));
                    gpt.gtior.write(|w| w.bits($gtior));
                    // Start counting up and force a deterministic 0% output. UDF must be
                    // asserted and then cleared for the direction to take effect.
                    let duty_off = 2u32 << $duty_shift;
                    gpt.gtuddtyc.write(|w| w.bits(duty_off | 3));
                    gpt.gtuddtyc.write(|w| w.bits(duty_off | 1));
                    gpt.gtcnt.write(|w| w.bits(0));
                    gpt.gtwp.write(|w| w.bits(0x0000_A501));
                }
                // Connect the pin only after the stopped timer has a deterministic low output.
                let pin = pin.into_alternate(PSEL_GPT);
                unsafe {
                    gpt.gtwp.write(|w| w.bits(0x0000_A500));
                    gpt.gtcr.write(|w| w.bits(((tpcs as u32) << 24) | 1)); // CST=1, start
                    gpt.gtwp.write(|w| w.bits(0x0000_A501));
                }
                Ok(Self {
                    gpt,
                    _pin: pin,
                    max_duty: (gtpr + 1) as u16,
                })
            }

            /// Releases the GPT handle and pin.
            pub fn release(self) -> (ra4m1::$ch_ty, Pin<$port, $pin_num, Alternate>) {
                unsafe {
                    self.gpt.gtwp.write(|w| w.bits(0x0000_A500));
                    self.gpt.gtcr.write(|w| w.bits(0));
                    self.gpt.gtior.write(|w| w.bits(0));
                    self.gpt.gtwp.write(|w| w.bits(0x0000_A501));
                }
                (self.gpt, self._pin)
            }
        }

        impl embedded_hal::pwm::ErrorType for $name {
            type Error = core::convert::Infallible;
        }

        impl embedded_hal::pwm::SetDutyCycle for $name {
            fn max_duty_cycle(&self) -> u16 {
                self.max_duty
            }
            fn set_duty_cycle(&mut self, duty: u16) -> Result<(), Self::Error> {
                let duty_mode = if duty == 0 {
                    2 // force 0%
                } else if duty >= self.max_duty {
                    3 // force 100%
                } else {
                    0 // compare-match operation
                };

                unsafe {
                    self.gpt.gtwp.write(|w| w.bits(0x0000_A500));
                    if duty_mode == 0 {
                        // With buffering, the transition occurs one timer count after the
                        // compare match, so N active counts require N-1 in the buffer.
                        self.gpt.$buffer.write(|w| w.bits(u32::from(duty) - 1));
                    }
                    self.gpt.gtuddtyc.modify(|r, w| {
                        w.bits(
                            (r.bits() & !(0b11 << $duty_shift))
                                | ((duty_mode as u32) << $duty_shift),
                        )
                    });
                    self.gpt.gtwp.write(|w| w.bits(0x0000_A501));
                }
                Ok(())
            }
        }
    };
}

#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D6 (P106) — GPT320 ch0 / GTIOC0B.
    d6, PwmD6, GPT320, '1', 6, true, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);
#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D3 (P104) — GPT321 ch1 / GTIOC1B. Exclusive with D11.
    d3, PwmD3, GPT321, '1', 4, true, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);
#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D11 (P109) — GPT321 ch1 / GTIOC1A. Exclusive with D3.
    d11, PwmD11, GPT321, '1', 9, true, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);
#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D5 (P102) — GPT162 ch2 / GTIOC2B.
    d5, PwmD5, GPT162, '1', 2, false, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);
#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D10 (P112) — GPT163 ch3 / GTIOC3B.
    d10, PwmD10, GPT163, '1', 12, false, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);
#[cfg(feature = "uno-r4-minima")]
pwm_channel!(
    /// PWM output on D9 (P303) — GPT167 ch7 / GTIOC7B.
    d9, PwmD9, GPT167, '3', 3, false, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);

#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D3 (P105) — GPT321 ch1 / GTIOC1A.
    d3, PwmD3, GPT321, '1', 5, true, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);
#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D5 (P107) — GPT320 ch0 / GTIOC0A.
    d5, PwmD5, GPT320, '1', 7, true, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);
#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D6 (P111) — GPT163 ch3 / GTIOC3A.
    d6, PwmD6, GPT163, '1', 11, false, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);
#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D9 (P303) — GPT167 ch7 / GTIOC7B.
    d9, PwmD9, GPT167, '3', 3, false, gtccrb, gtccre, 24, GTIOR_OUTPUT_B
);
#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D10 (P103) — GPT162 ch2 / GTIOC2A.
    d10, PwmD10, GPT162, '1', 3, false, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);
#[cfg(feature = "uno-r4-wifi")]
pwm_channel!(
    /// PWM output on D11 (P411) — GPT166 ch6 / GTIOC6A.
    d11, PwmD11, GPT166, '4', 11, false, gtccra, gtccrc, 16, GTIOR_OUTPUT_A
);

#[cfg(test)]
mod tests {
    use super::compute_period;

    #[test]
    fn computes_common_periods() {
        assert_eq!(compute_period(48_000_000, 1_000), Some((0, 47_999)));
        assert_eq!(compute_period(48_000_000, 1), Some((5, 46_874)));
    }

    #[test]
    fn rounds_to_the_nearest_period() {
        assert_eq!(compute_period(48_000_000, 44_100), Some((0, 1_087)));
    }

    #[test]
    fn rejects_zero_and_unrepresentable_frequencies() {
        assert_eq!(compute_period(48_000_000, 0), None);
        assert_eq!(compute_period(48_000_000, 32_000_000), None);
        assert_eq!(compute_period(48_000_000, 40_000_000), None);
    }
}
