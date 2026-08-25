# arduino-uno-r4-hal

Arduino Uno R4 Minima / WiFi (Renesas RA4M1 / Cortex-M4F) 向けの [embedded-hal](https://github.com/rust-embedded/embedded-hal) 1.0 実装。[`ra4m1`](https://crates.io/crates/ra4m1) PAC の上に構築する。

## 特長

- **クロック**: 起動時に HOCO を 48MHz に初期化し系統クロックへ設定 (ICLK 48 / PCLKB 24 / PCLKD 48 / FCLK 24 MHz)
- **Delay**: SysTick ベースのブロッキング遅延 (`embedded_hal::delay::DelayNs`)
- **GPIO**: 入力 / プルアップ入力 / 出力 (プッシュプル・オープンドレイン)。プルアップ/OD は `PmnPFS` 経由で正しく設定。出力 set/clear は `POSR`/`PORR` でアトミック。型状態 + 所有権モデル
- **UART** (`Serial1` = D0/D1): 調歩同期 8N1。`embedded-io` / `embedded-hal-nb` / `core::fmt::Write`
- **ADC** (A0–A5): 14bit 逐次比較
- **PWM** (`~` ピン: D3/D5/D6/D9/D10/D11): GPT 鋸波 PWM。`embedded_hal::pwm::SetDutyCycle`

## 使い方

エントリポイントは [`Peripherals::take()`]。Cortex-M / PAC ペリフェラルを一度だけ消費し、
クロックを初期化したうえで各リソースを配る。

```rust
#![no_std]
#![no_main]

use arduino_uno_r4_hal::Peripherals;
use embedded_hal::delay::DelayNs;
use panic_halt as _;

#[cortex_m_rt::entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();
    let mut delay = p.delay;             // SysTick 遅延
    let mut led = p.pins.d13.into_output(); // D13 = オンボード LED

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
let mut serial = Serial::new(p.sci2, p.pins.d1, p.pins.d0, 115_200, &clocks).unwrap();
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

以下は既定の Minima feature 向け。WiFi では D6 に `p.gpt.gpt163` を使う。

```rust
use arduino_uno_r4_hal::{pwm::PwmD6, Peripherals};
use embedded_hal::pwm::SetDutyCycle;

let p = Peripherals::take().unwrap();
let mut pwm = PwmD6::new(p.gpt.gpt320, p.pins.d6, 1_000, &p.clocks).unwrap(); // 1kHz
pwm.set_duty_cycle_percent(25).unwrap();
```

## ボードの選択

既定の feature は `uno-r4-minima`。Minima と WiFi では Arduino ヘッダの一部が異なる RA4M1 ピンへ接続されているため、必ず片方だけを有効にする。

```toml
# UNO R4 Minima (既定)
arduino-uno-r4-hal = "0.2"

# UNO R4 WiFi
arduino-uno-r4-hal = { version = "0.2", default-features = false, features = ["uno-r4-wifi"] }
```

このリポジトリで WiFi 向けにビルドする場合は `cargo build --no-default-features --features uno-r4-wifi --examples` を使う。

## ピン対応表 (Arduino → RA4M1)

### UNO R4 Minima

| Arduino | RA4M1 | 機能         |     | Arduino | RA4M1 | 機能            |
| ------- | ----- | ------------ | --- | ------- | ----- | --------------- |
| D0      | P301  | RX (SCI2)    |     | D10     | P112  | PWM(GTIOC3B)    |
| D1      | P302  | TX (SCI2)    |     | D11     | P109  | PWM(GTIOC1A)    |
| D2      | P105  |              |     | D12     | P110  |                 |
| D3      | P104  | PWM(GTIOC1B) |     | D13     | P111  | LED / PWM 不可  |
| D4      | P103  |              |     | A0      | P014  | ADC AN009       |
| D5      | P102  | PWM(GTIOC2B) |     | A1      | P000  | ADC AN000       |
| D6      | P106  | PWM(GTIOC0B) |     | A2      | P001  | ADC AN001       |
| D7      | P107  |              |     | A3      | P002  | ADC AN002       |
| D8      | P304  |              |     | A4      | P101  | SDA / ADC AN021 |
| D9      | P303  | PWM(GTIOC7B) |     | A5      | P100  | SCL / ADC AN022 |

> **PWM の注意**: D3 と D11 は同一 GPT チャネル (GPT321) を共有するため同時には片方のみ使用可能。

### UNO R4 WiFi

| Arduino | RA4M1 | 機能         |     | Arduino | RA4M1 | 機能            |
| ------- | ----- | ------------ | --- | ------- | ----- | --------------- |
| D0      | P301  | RX (SCI2)    |     | D10     | P103  | PWM(GTIOC2A)    |
| D1      | P302  | TX (SCI2)    |     | D11     | P411  | PWM(GTIOC6A)    |
| D2      | P104  |              |     | D12     | P410  |                 |
| D3      | P105  | PWM(GTIOC1A) |     | D13     | P102  | LED / PWM 不可  |
| D4      | P106  |              |     | A0      | P014  | ADC AN009       |
| D5      | P107  | PWM(GTIOC0A) |     | A1      | P000  | ADC AN000       |
| D6      | P111  | PWM(GTIOC3A) |     | A2      | P001  | ADC AN001       |
| D7      | P112  |              |     | A3      | P002  | ADC AN002       |
| D8      | P304  |              |     | A4      | P101  | SDA / ADC AN021 |
| D9      | P303  | PWM(GTIOC7B) |     | A5      | P100  | SCL / ADC AN022 |

## ビルド

ターゲット `thumbv7em-none-eabihf` (`.cargo/config.toml` で既定設定済み):

```bash
cargo build --examples
cargo test --target x86_64-unknown-linux-gnu --lib
```

書き込み・実行には [`probe-rs`](https://probe.rs/) を使用 (`.cargo/config.toml` の runner):

```bash
cargo run --example blink   # probe-rs run --chip RA4M1
```

## 実機スモークテスト

> 本 HAL は実機未検証 (作者が端末を入手後に確認予定)。以下で各機能を確認できる。

- **blink**: D13 の LED が 1Hz で点滅 (クロック 48MHz 初期化が効き、周期が正しいこと)
- **button**: D2 をプルアップ入力にし、GND 短絡で LED が反応 (PFS 経由プルアップの確認)
- **serial**: 115200 8N1 で TX に文字列出力、ループバックで RX 受信
- **adc**: A0 に電圧を与え `analogRead` 相当の値を UART で確認
- **pwm**: D6 のデューティをロジックアナライザ/テスタで確認

## ライセンス

MIT OR Apache-2.0
