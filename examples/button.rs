#![no_std]
#![no_main]

use arduino_uno_r4_hal::Peripherals;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let button = p.pins.d2.into_input_pullup();
    let mut led = p.pins.d13.into_output();

    loop {
        // Pull-up: pressed (shorted to GND) reads low.
        if button.is_low() {
            led.set_high();
        } else {
            led.set_low();
        }
    }
}
