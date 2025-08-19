#![no_std]
#![no_main]

use arduino_uno_r4_hal::{adc::Adc, serial::Serial, Peripherals};
use core::fmt::Write;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = p.clocks;
    let mut delay = p.delay;

    let mut adc = Adc::new(p.adc140, &clocks);
    let a0 = p.pins.a0.into_analog();
    let mut serial = Serial::new(p.sci2, p.pins.d1, p.pins.d0, 115_200, &clocks);

    loop {
        let value = adc.read(&a0); // 0..=16383
        writeln!(serial, "A0 = {} / 16383", value).ok();
        delay.delay_ms(500);
    }
}
