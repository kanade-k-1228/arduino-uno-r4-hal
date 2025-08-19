//! Blocking delay using SysTick (the Cortex-M core timer).
//!
//! The previous implementation busy-waited with a `nop` loop, which wasn't cycle-accurate
//! due to compiler optimizations and pipeline effects. This one drives SysTick from the
//! core clock (ICLK = [`Clocks::sysclk`]) and counts the exact number of cycles requested.

use crate::clock::Clocks;
use cortex_m::peripheral::{syst::SystClkSource, SYST};
use embedded_hal::delay::DelayNs;

/// Maximum SysTick reload value (24-bit).
const MAX_RELOAD: u32 = 0x00FF_FFFF;

/// SysTick-based blocking delay provider.
pub struct Delay {
    syst: SYST,
    sysclk_hz: u32,
}

impl Delay {
    /// Builds a delay provider from SysTick and the clock configuration.
    ///
    /// Sets SysTick's clock source to the core clock. `clocks` is the return value of
    /// [`crate::clock::init`].
    pub fn new(mut syst: SYST, clocks: &Clocks) -> Self {
        syst.set_clock_source(SystClkSource::Core);
        Self {
            syst,
            sysclk_hz: clocks.sysclk().to_Hz(),
        }
    }

    /// Releases SysTick back to the caller.
    pub fn free(self) -> SYST {
        self.syst
    }

    /// Busy-waits for the given number of cycles, splitting into chunks bounded by the
    /// 24-bit reload limit.
    ///
    /// Takes a u64 because long delays (e.g. `delay_us(u32::MAX)`) can need more cycles
    /// than fit in a u32 (about 89 seconds at 48MHz).
    fn delay_cycles(&mut self, cycles: u64) {
        let mut remaining = cycles;
        while remaining != 0 {
            // `current` is always >= 1 here, so reload never hits 0.
            let current = remaining.min(MAX_RELOAD as u64) as u32;
            self.syst.set_reload(current);
            self.syst.clear_current();
            self.syst.enable_counter();
            remaining -= current as u64;
            while !self.syst.has_wrapped() {}
            self.syst.disable_counter();
        }
    }
}

impl DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        // Round up to guarantee at least `ns` nanoseconds.
        let cycles = (ns as u64 * self.sysclk_hz as u64).div_ceil(1_000_000_000);
        self.delay_cycles(cycles);
    }

    fn delay_us(&mut self, us: u32) {
        let cycles = (us as u64 * self.sysclk_hz as u64).div_ceil(1_000_000);
        self.delay_cycles(cycles);
    }

    fn delay_ms(&mut self, ms: u32) {
        let cycles = (ms as u64 * self.sysclk_hz as u64).div_ceil(1_000);
        self.delay_cycles(cycles);
    }
}
