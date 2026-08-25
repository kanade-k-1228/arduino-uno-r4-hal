#![no_std]
#![no_main]

use arduino_uno_r4_hal::{serial::Serial, Peripherals};
use core::fmt::Write;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let clocks = p.clocks;
    let mut delay = p.delay;

    // D1 = TX, D0 = RX
    let mut serial = Serial::new(p.sci2, p.pins.d1, p.pins.d0, 115_200, &clocks).unwrap();

    let mut count: u32 = 0;
    loop {
        writeln!(serial, "hello from uno r4: {}", count).ok();
        count = count.wrapping_add(1);
        delay.delay_ms(1000);
    }
}
