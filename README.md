# arduino-uno-r4-hal

Hardware Abstraction Layer (HAL) for Arduino Uno R4 (RA4M1) microcontroller, implementing the embedded-hal traits.

## Features

- **Delay**: Microsecond and millisecond delays using busy-wait loops
- **GPIO**: Digital input/output with support for:
  - Input mode (with optional pull-up)
  - Output mode (push-pull and open-drain)
  - Pin state reading and writing
  - Pin toggling

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
arduino-uno-r4-hal = "0.1.0"
```

## Examples

### Blink LED

```rust
#![no_std]
#![no_main]

use arduino_uno_r4_hal::gpio::{Pin, Output};
use arduino_uno_r4_hal::delay::Delay;
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
```

### Read Button

```rust
#![no_std]
#![no_main]

use arduino_uno_r4_hal::gpio::{Pin, InputPullUp, Output};
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
```

## Building

```bash
cargo build --examples
```

## License

MIT OR Apache-2.0
