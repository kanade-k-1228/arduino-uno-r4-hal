//! PWM output (GPT timer, sawtooth PWM mode).
//!
//! Corresponds to the Arduino PWM pins (marked `~`). Each pin is wired to a fixed GPT
//! channel/output:
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
//! D3 and D11 share the same channel (GPT321), so only one of them can be used at a time
//! (the type system enforces this too, since `gpt.gpt321` can only be consumed once).
//!
//! Implements [`embedded_hal::pwm::SetDutyCycle`]. Frequency is fixed at construction time.
//!
//! ```ignore
//! use embedded_hal::pwm::SetDutyCycle;
//! let mut led = Pwm::d6(p.gpt.gpt320, p.pins.d6, 1_000, &clocks); // 1kHz
//! led.set_duty_cycle_percent(25).unwrap();
//! ```

use crate::clock::Clocks;
use crate::gpio::{Alternate, Pin};

/// PSEL value that assigns a pin's GTIOC to PWM (RA4M1 I/O port GPT(GTIOC) selection = 0b00011).
const PSEL_GPT: u8 = 0b0_0011;

/// PAC handles for the GPT channels used by PWM. Provided by [`crate::Peripherals::take`].
#[allow(missing_docs)]
pub struct GptChannels {
    pub gpt320: ra4m1::GPT320, // ch0 -> D6
    pub gpt321: ra4m1::GPT321, // ch1 -> D3 / D11 (exclusive)
    pub gpt162: ra4m1::GPT162, // ch2 -> D5
    pub gpt163: ra4m1::GPT163, // ch3 -> D10
    pub gpt167: ra4m1::GPT167, // ch7 -> D9
}

/// Releases the module stop for the given GPT channel group.
/// Channels 0..3 use MSTPCRD.MSTPD5, channels 4..7 use MSTPD6.
fn enable_gpt(group_d5: bool) {
    critical_section::with(|_| unsafe {
        let system = &*ra4m1::SYSTEM::PTR;
        let mstp = &*ra4m1::MSTP::PTR;
        system.prcr.write(|w| w.bits(0xA502));
        if group_d5 {
            mstp.mstpcrd.modify(|_, w| w.mstpd5()._0());
        } else {
            mstp.mstpcrd.modify(|_, w| w.mstpd6()._0());
        }
        system.prcr.write(|w| w.bits(0xA500));
    });
}

/// Finds the (prescaler, GTPR) pair that realizes `freq` Hz.
///
/// `GTPR+1 = PCLKD / (prescaler * freq)`. Picks the smallest prescaler for which GTPR+1
/// fits in a u16 (the embedded-hal duty cycle limit).
fn compute_period(pclkd: u32, freq: u32) -> (u8, u32) {
    // (TPCS value, divisor)
    const DIVS: [(u8, u32); 6] = [(0, 1), (1, 4), (2, 16), (3, 64), (4, 256), (5, 1024)];
    let mut i = 0;
    while i < DIVS.len() {
        let (tpcs, div) = DIVS[i];
        let ticks = pclkd / (div * freq.max(1));
        if ticks <= 0xFFFF {
            let t = if ticks < 2 { 2 } else { ticks };
            return (tpcs, t - 1);
        }
        i += 1;
    }
    (5, 0xFFFE)
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
        $group_d5:expr, $ccr:ident, $gtior:expr
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
            ) -> Self {
                enable_gpt($group_d5);
                let pin = pin.into_alternate(PSEL_GPT);
                let (tpcs, gtpr) = compute_period(clocks.pclkd().to_Hz(), freq);
                unsafe {
                    gpt.gtwp.write(|w| w.bits(0x0000_A500)); // unlock write protection
                    gpt.gtcr.write(|w| w.bits((tpcs as u32) << 24)); // MD=sawtooth(0), CST=stopped, prescaler
                    gpt.gtuddtyc.write(|w| w.bits(0x0000_0003)); // UD=1, UDF=1 (count up)
                    gpt.gtpr.write(|w| w.bits(gtpr));
                    gpt.$ccr.write(|w| w.bits(0)); // 0% duty
                    gpt.gtior.write(|w| w.bits($gtior));
                    gpt.gtcnt.write(|w| w.bits(0));
                    gpt.gtcr.write(|w| w.bits(((tpcs as u32) << 24) | 1)); // CST=1, start
                }
                Self {
                    gpt,
                    _pin: pin,
                    max_duty: (gtpr + 1) as u16,
                }
            }

            /// Releases the GPT handle and pin.
            pub fn release(self) -> (ra4m1::$ch_ty, Pin<$port, $pin_num, Alternate>) {
                unsafe { self.gpt.gtcr.write(|w| w.bits(0)) }; // stop
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
                unsafe { self.gpt.$ccr.write(|w| w.bits(duty as u32)) };
                Ok(())
            }
        }
    };
}

pwm_channel!(
    /// PWM output on D6 (P106) — GPT320 ch0 / GTIOC0B.
    d6, PwmD6, GPT320, '1', 6, true, gtccrb, GTIOR_OUTPUT_B
);
pwm_channel!(
    /// PWM output on D3 (P104) — GPT321 ch1 / GTIOC1B. Exclusive with D11.
    d3, PwmD3, GPT321, '1', 4, true, gtccrb, GTIOR_OUTPUT_B
);
pwm_channel!(
    /// PWM output on D11 (P109) — GPT321 ch1 / GTIOC1A. Exclusive with D3.
    d11, PwmD11, GPT321, '1', 9, true, gtccra, GTIOR_OUTPUT_A
);
pwm_channel!(
    /// PWM output on D5 (P102) — GPT162 ch2 / GTIOC2B.
    d5, PwmD5, GPT162, '1', 2, true, gtccrb, GTIOR_OUTPUT_B
);
pwm_channel!(
    /// PWM output on D10 (P112) — GPT163 ch3 / GTIOC3B.
    d10, PwmD10, GPT163, '1', 12, true, gtccrb, GTIOR_OUTPUT_B
);
pwm_channel!(
    /// PWM output on D9 (P303) — GPT167 ch7 / GTIOC7B.
    d9, PwmD9, GPT167, '3', 3, false, gtccrb, GTIOR_OUTPUT_B
);
