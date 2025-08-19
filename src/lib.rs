//! embedded-hal implementation for the Arduino Uno R4 (Renesas RA4M1 / Cortex-M4F).
//!
//! [`Peripherals::take`] is the entry point. It consumes the `ra4m1` PAC and Cortex-M core
//! peripherals once, initializes the clock to 48MHz, and hands out:
//!
//! - [`Clocks`] — resolved clock frequencies
//! - [`Delay`] — SysTick-based blocking delay
//! - [`gpio::Pins`] — Arduino header pins (`d0`..`d13`, `a0`..`a5`)
//! - PAC handles for UART / ADC / PWM (moved into their respective drivers)
//!
//! ```ignore
//! let p = arduino_uno_r4_hal::Peripherals::take().unwrap();
//! let mut delay = p.delay;
//! let mut led = p.pins.d13.into_output();
//! loop {
//!     led.toggle();
//!     delay.delay_ms(500);
//! }
//! ```

#![no_std]

mod setting;

pub mod adc;
pub mod clock;
pub mod delay;
pub mod gpio;
pub mod pwm;
pub mod serial;
pub mod time;

pub use clock::Clocks;
pub use delay::Delay;
pub use gpio::{
    Alternate, Analog, Input, InputPullUp, Output, OutputOpenDrain, Pin, PinState, Pins,
};

/// Initialized top-level peripherals.
pub struct Peripherals {
    /// Resolved clock frequencies (HOCO 48MHz).
    pub clocks: Clocks,
    /// SysTick-based delay provider.
    pub delay: Delay,
    /// Arduino header pins.
    pub pins: gpio::Pins,
    /// SCI2 handle for UART (Serial1 = D0/D1). Pass to [`serial::Serial::new`].
    pub sci2: ra4m1::SCI2,
    /// ADC140 handle for the ADC (A0..A5). Pass to [`adc::Adc::new`].
    pub adc140: ra4m1::ADC140,
    /// GPT channel handles for PWM. Pass to the `pwm` module's drivers (`PwmD6`, etc.).
    pub gpt: pwm::GptChannels,
}

impl Peripherals {
    /// Takes all peripherals and returns them with the clock initialized.
    ///
    /// Backed by `cortex_m::Peripherals::take()`, so this only succeeds once; a second
    /// call, or a prior direct take of the Cortex-M peripherals, returns `None`.
    ///
    /// The `ra4m1` PAC doesn't expose `Peripherals::take()` without the `critical-section`
    /// feature, so `ra4m1::Peripherals::steal()` is used once, guarded by the check above.
    pub fn take() -> Option<Self> {
        // One-shot guard: returns None if already taken.
        let cp = cortex_m::Peripherals::take()?;
        // Safe: passing the guard above means device peripherals haven't been taken yet.
        let dp = unsafe { ra4m1::Peripherals::steal() };

        let clocks = clock::init(dp.SYSTEM);
        let delay = Delay::new(cp.SYST, &clocks);
        let pins = gpio::Pins::new(dp.PORT0, dp.PORT1, dp.PORT3, dp.PFS, dp.PMISC);

        Some(Self {
            clocks,
            delay,
            pins,
            sci2: dp.SCI2,
            adc140: dp.ADC140,
            gpt: pwm::GptChannels {
                gpt320: dp.GPT320,
                gpt321: dp.GPT321,
                gpt162: dp.GPT162,
                gpt163: dp.GPT163,
                gpt167: dp.GPT167,
            },
        })
    }
}
