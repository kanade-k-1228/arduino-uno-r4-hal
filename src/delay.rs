use cortex_m::asm;
use embedded_hal::delay::DelayNs;

use crate::time::{ClockConfig, SYSCLK_FREQ};

pub struct Delay {
    sysclk_freq: u32,
}

impl Delay {
    pub fn new() -> Self {
        Self {
            sysclk_freq: SYSCLK_FREQ.to_Hz(),
        }
    }

    pub fn new_with_config(config: &ClockConfig) -> Self {
        Self {
            sysclk_freq: config.sysclk.to_Hz(),
        }
    }

    #[inline]
    fn delay_cycles(&self, cycles: u32) {
        let mut remaining = cycles;
        while remaining > 0 {
            asm::nop();
            remaining = remaining.saturating_sub(1);
        }
    }
}

impl Default for Delay {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayNs for Delay {
    fn delay_ns(&mut self, ns: u32) {
        let cycles = (ns as u64 * self.sysclk_freq as u64 / 1_000_000_000) as u32;
        self.delay_cycles(cycles.max(1));
    }

    fn delay_us(&mut self, us: u32) {
        let cycles = (us as u64 * self.sysclk_freq as u64 / 1_000_000) as u32;
        self.delay_cycles(cycles.max(1));
    }

    fn delay_ms(&mut self, ms: u32) {
        for _ in 0..ms {
            self.delay_us(1000);
        }
    }
}

pub fn delay_ms(ms: u32) {
    let mut delay = Delay::new();
    delay.delay_ms(ms);
}

pub fn delay_us(us: u32) {
    let mut delay = Delay::new();
    delay.delay_us(us);
}
