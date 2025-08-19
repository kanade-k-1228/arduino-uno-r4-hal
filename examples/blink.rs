#![no_std]
#![no_main]

use arduino_uno_r4_hal::{
    gpio::{Output, Pin},
    Delay,
};
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let mut led = Pin::<'1', 11, Output>::new();
    let mut delay = Delay::new();

    loop {
        led.set_high();
        delay.delay_ms(500);

        led.set_low();
        delay.delay_ms(500);
    }
}
