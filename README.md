# arduino-uno-r4-hal

An [embedded-hal](https://github.com/rust-embedded/embedded-hal) 1.0 implementation for the Arduino Uno R4 (Renesas RA4M1 / Cortex-M4F), built on top of the [`ra4m1`](https://crates.io/crates/ra4m1) PAC.

日本語版は [README.jp.md](README.jp.md) を参照してください。

## Features

- **Clock**: Initializes HOCO to 48MHz on startup and configures the system clocks (ICLK 48 / PCLKB 24 / PCLKD 48 / FCLK 24 MHz)
- **Delay**: SysTick-based blocking delay (`embedded_hal::delay::DelayNs`)
- **GPIO**: Input / pull-up input / output (push-pull and open-drain). Pull-up/OD are configured correctly via `PmnPFS`. Output set/clear is atomic via `POSR`/`PORR`. Type-state + ownership model
- **UART** (`Serial1` = D0/D1): Asynchronous 8N1. `embedded-io` / `embedded-hal-nb` / `core::fmt::Write`
- **ADC** (A0–A5): 14-bit successive approximation
- **PWM** (`~` pins: D3/D5/D6/D9/D10/D11): GPT sawtooth PWM. `embedded_hal::pwm::SetDutyCycle`

## Usage

The entry point is [`Peripherals::take()`]. It consumes the Cortex-M / PAC peripherals exactly once,
initializes the clocks, and then hands out each resource.

```rust
#![no_std]
#![no_main]

use arduino_uno_r4_hal::Peripherals;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut delay = p.delay;             // SysTick delay
    let mut led = p.pins.d13.into_output(); // D13 = onboard LED

    loop {
        led.toggle();
        delay.delay_ms(500);
    }
}
```

### UART

```rust
use arduino_uno_r4_hal::{serial::Serial, Peripherals};
use core::fmt::Write;

let p = Peripherals::take().unwrap();
let clocks = p.clocks;
// D1 = TX, D0 = RX, 115200 8N1
let mut serial = Serial::new(p.sci2, p.pins.d1, p.pins.d0, 115_200, &clocks);
writeln!(serial, "hello").ok();
```

### ADC

```rust
use arduino_uno_r4_hal::{adc::Adc, Peripherals};

let p = Peripherals::take().unwrap();
let mut adc = Adc::new(p.adc140, &p.clocks);
let a0 = p.pins.a0.into_analog();
let value: u16 = adc.read(&a0); // 0..=16383
```

### PWM

```rust
use arduino_uno_r4_hal::{pwm::PwmD6, Peripherals};
use embedded_hal::pwm::SetDutyCycle;

let p = Peripherals::take().unwrap();
let mut pwm = PwmD6::new(p.gpt.gpt320, p.pins.d6, 1_000, &p.clocks); // 1kHz
pwm.set_duty_cycle_percent(25).unwrap();
```

## Pin mapping (Arduino → RA4M1)

| Arduino | RA4M1 | Function | | Arduino | RA4M1 | Function |
|---|---|---|---|---|---|---|
| D0  | P301 | RX (SCI2)        | | D10 | P112 | PWM(GTIOC3B) |
| D1  | P302 | TX (SCI2)        | | D11 | P109 | PWM(GTIOC1A) |
| D2  | P105 |                  | | D12 | P110 | |
| D3  | P104 | PWM(GTIOC1B)     | | D13 | P111 | LED / no PWM |
| D4  | P103 |                  | | A0  | P014 | ADC AN009 |
| D5  | P102 | PWM(GTIOC2B)     | | A1  | P000 | ADC AN000 |
| D6  | P106 | PWM(GTIOC0B)     | | A2  | P001 | ADC AN001 |
| D7  | P107 |                  | | A3  | P002 | ADC AN002 |
| D8  | P304 |                  | | A4  | P101 | SDA / ADC AN021 |
| D9  | P303 | PWM(GTIOC7B)     | | A5  | P100 | SCL / ADC AN022 |

> **PWM note**: D3 and D11 share the same GPT channel (GPT321), so only one of them can be used at a time.

## Build

Target `thumbv7em-none-eabihf` (already the default in `.cargo/config.toml`):

```bash
cargo build --examples
```

Use [`probe-rs`](https://probe.rs/) to flash and run (configured as the runner in `.cargo/config.toml`):

```bash
cargo run --example blink   # probe-rs run --chip RA4M1
```

## On-device smoke tests

> This HAL has not yet been verified on real hardware (the author plans to test it once the board is available). The following checks can be used to verify each feature.

- **blink**: The D13 LED blinks at 1Hz (confirms the 48MHz clock init worked and the period is correct)
- **button**: D2 configured as pull-up input; shorting to GND makes the LED react (confirms pull-up via PFS)
- **serial**: A string is written to TX at 115200 8N1, and received back on RX via loopback
- **adc**: Apply a voltage to A0 and check an `analogRead`-equivalent value over UART
- **pwm**: Check the duty cycle on D6 with a logic analyzer/multimeter

## License

MIT OR Apache-2.0
