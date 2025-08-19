use fugit::{HertzU32, MicrosDurationU32, MillisDurationU32};

pub const SYSCLK_FREQ: HertzU32 = HertzU32::MHz(48);

pub type Micros = MicrosDurationU32;
pub type Millis = MillisDurationU32;

#[derive(Clone, Copy, Debug)]
pub struct ClockConfig {
    pub sysclk: HertzU32,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            sysclk: SYSCLK_FREQ,
        }
    }
}
