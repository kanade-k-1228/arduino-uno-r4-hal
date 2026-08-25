//! Blocking delay using SysTick (the Cortex-M core timer).
//!
//! The previous implementation busy-waited with a `nop` loop, which wasn't cycle-accurate
//! due to compiler optimizations and pipeline effects. This one drives SysTick from the
//! core clock (ICLK = [`Clocks::sysclk`]) and counts the exact number of cycles requested.

use crate::clock::Clocks;
use cortex_m::peripheral::{syst::SystClkSource, SYST};
use embedded_hal::delay::DelayNs;

/// Maximum number of cycles in one SysTick run (24-bit reload plus the zero count).
const MAX_TICKS: u32 = 0x0100_0000;

#[inline]
fn systick_chunk(remaining: u64) -> (u32, u32) {
    debug_assert!(remaining >= 2);
    let ticks = remaining.min(MAX_TICKS as u64) as u32;
    (ticks - 1, ticks)
}

#[inline]
fn duration_to_cycles(amount: u32, clock_hz: u32, units_per_second: u64) -> u64 {
    (u64::from(amount) * u64::from(clock_hz)).div_ceil(units_per_second)
}

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
        // SysTick cannot provide a useful wrap wait with RELOAD=0. Function and loop
        // overhead already exceed a single requested core cycle, so no timer run is
        // needed for a final one-cycle remainder.
        while remaining >= 2 {
            // SysTick runs for RELOAD + 1 cycles, so N cycles require RELOAD=N-1.
            let (reload, current) = systick_chunk(remaining);
            self.syst.set_reload(reload);
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
        let cycles = duration_to_cycles(ns, self.sysclk_hz, 1_000_000_000);
        self.delay_cycles(cycles);
    }

    fn delay_us(&mut self, us: u32) {
        let cycles = duration_to_cycles(us, self.sysclk_hz, 1_000_000);
        self.delay_cycles(cycles);
    }

    fn delay_ms(&mut self, ms: u32) {
        let cycles = duration_to_cycles(ms, self.sysclk_hz, 1_000);
        self.delay_cycles(cycles);
    }
}

#[cfg(test)]
mod tests {
    use super::{duration_to_cycles, systick_chunk, MAX_TICKS};

    #[test]
    fn converts_durations_and_rounds_up() {
        assert_eq!(duration_to_cycles(0, 48_000_000, 1_000_000_000), 0);
        assert_eq!(duration_to_cycles(1, 48_000_000, 1_000_000_000), 1);
        assert_eq!(duration_to_cycles(1, 48_000_000, 1_000_000), 48);
        assert_eq!(duration_to_cycles(1, 48_000_000, 1_000), 48_000);
    }

    #[test]
    fn translates_ticks_to_reload_without_an_off_by_one() {
        assert_eq!(systick_chunk(2), (1, 2));
        assert_eq!(systick_chunk(MAX_TICKS as u64), (0x00ff_ffff, MAX_TICKS));
        assert_eq!(
            systick_chunk(MAX_TICKS as u64 + 1),
            (0x00ff_ffff, MAX_TICKS)
        );
    }
}
