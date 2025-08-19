#![no_std]
#![no_main]

use arduino_uno_r4_hal::Peripherals;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut delay = p.delay;
    let mut led = p.pins.d13.into_output();

    loop {
        led.set_high();
        delay.delay_ms(500);

        led.set_low();
        delay.delay_ms(500);
    }
}
