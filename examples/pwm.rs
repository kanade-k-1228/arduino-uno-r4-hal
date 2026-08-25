#![no_std]
#![no_main]

use arduino_uno_r4_hal::{pwm::PwmD6, Peripherals};
use embedded_hal::delay::DelayNs;
use embedded_hal::pwm::SetDutyCycle;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = p.clocks;
    let mut delay = p.delay;

    #[cfg(feature = "uno-r4-minima")]
    let mut pwm = PwmD6::new(p.gpt.gpt320, p.pins.d6, 1_000, &clocks).unwrap();
    #[cfg(feature = "uno-r4-wifi")]
    let mut pwm = PwmD6::new(p.gpt.gpt163, p.pins.d6, 1_000, &clocks).unwrap();

    loop {
        for duty in 0..=100u8 {
            pwm.set_duty_cycle_percent(duty).ok();
            delay.delay_ms(10);
        }
        for duty in (0..=100u8).rev() {
            pwm.set_duty_cycle_percent(duty).ok();
            delay.delay_ms(10);
        }
    }
}
