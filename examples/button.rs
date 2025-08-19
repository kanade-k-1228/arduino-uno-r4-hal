#![no_std]
#![no_main]

use arduino_uno_r4_hal::gpio::{InputPullUp, Output, Pin};
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let button = Pin::<'0', 2, InputPullUp>::new();
    let mut led = Pin::<'1', 11, Output>::new();

    loop {
        if button.is_low() {
            led.set_high();
        } else {
            led.set_low();
        }
    }
}
