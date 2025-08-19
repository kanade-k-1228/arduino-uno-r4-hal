//! Clock generation circuit (CGC) initialization.
//!
//! On reset, the RA4M1 starts from the mid-speed on-chip oscillator MOCO (8MHz) as the
//! system clock (`SCKSCR.CKSEL` resets to MOCO). The Arduino Uno R4 runs the high-speed
//! on-chip oscillator HOCO at 48MHz instead, so [`init`] does the following:
//!
//! 1. Unlock the protect register `PRCR` (allow writes to clock/low-power-mode registers)
//! 2. Switch the operating power control mode to high-speed (`OPCCR.OPCM`) and wait for
//!    the transition via `OPCMTSF`
//! 3. Start HOCO (`HOCOCR.HCSTP=0`) and wait for oscillation stability via `OSCSF.HOCOSF`
//! 4. Insert one flash wait cycle (`MEMWAIT=1`), required above ICLK>32MHz
//! 5. Set the clock dividers (`SCKDIVCR`): ICLK/1, PCLKA/1, PCLKB/2, PCLKC/1, PCLKD/1, FCLK/2
//! 6. Switch the system clock source to HOCO (`SCKSCR.CKSEL=HOCO`)
//! 7. Relock `PRCR`
//!
//! This matches the clock init sequence used by the Arduino Uno R4 bootloader.

use fugit::HertzU32;
use ra4m1::SYSTEM;

/// HOCO oscillation frequency (48MHz), the Arduino Uno R4's system clock source.
pub const HOCO_FREQ: HertzU32 = HertzU32::MHz(48);

/// `PRCR` unlock key: PRKEY=0xA5, PRC0=1 (clock generation registers writable), PRC1=1 (low-power mode).
const PRCR_UNLOCK: u16 = 0xA503;
/// `PRCR` relock (all permission bits cleared).
const PRCR_LOCK: u16 = 0xA500;

/// Resolved clock frequencies. Peripherals use these to derive baud rates and timer periods.
#[derive(Clone, Copy, Debug)]
pub struct Clocks {
    sysclk: HertzU32,
    pclka: HertzU32,
    pclkb: HertzU32,
    pclkc: HertzU32,
    pclkd: HertzU32,
    fclk: HertzU32,
}

impl Clocks {
    /// System clock ICLK (= CPU / SysTick clock).
    #[inline]
    pub fn sysclk(&self) -> HertzU32 {
        self.sysclk
    }
    /// ICLK, an alias for [`Clocks::sysclk`].
    #[inline]
    pub fn iclk(&self) -> HertzU32 {
        self.sysclk
    }
    /// Peripheral clock A (PCLKA): upstream clock for SCI, etc.
    #[inline]
    pub fn pclka(&self) -> HertzU32 {
        self.pclka
    }
    /// Peripheral clock B (PCLKB): reference clock for SCI baud rate / IIC, etc.
    #[inline]
    pub fn pclkb(&self) -> HertzU32 {
        self.pclkb
    }
    /// Peripheral clock C (PCLKC).
    #[inline]
    pub fn pclkc(&self) -> HertzU32 {
        self.pclkc
    }
    /// Peripheral clock D (PCLKD): reference clock for GPT / ADC.
    #[inline]
    pub fn pclkd(&self) -> HertzU32 {
        self.pclkd
    }
    /// Flash clock FCLK.
    #[inline]
    pub fn fclk(&self) -> HertzU32 {
        self.fclk
    }
}

/// Initializes the CGC with HOCO 48MHz as the system clock and returns the resolved frequencies.
///
/// Consumes the `SYSTEM` peripheral, since clock setup only happens once at startup.
pub fn init(system: SYSTEM) -> Clocks {
    // 1. Unlock (allow writes to CGC / LPM registers).
    system.prcr.write(|w| unsafe { w.bits(PRCR_UNLOCK) });

    // 2. Switch to high-speed operating mode; wait for the transition (OPCMTSF=0).
    system.opccr.modify(|_, w| w.opcm()._00());
    while system.opccr.read().opcmtsf().is_1() {}

    // 3. Start HOCO and wait for oscillation stability (HOCOSF=1).
    system.hococr.modify(|_, w| w.hcstp()._0());
    while system.oscsf.read().hocosf().is_0() {}

    // 4. Insert a flash wait cycle (required above ICLK>32MHz).
    system.memwait.modify(|_, w| w.memwait()._1());

    // 5. Set dividers: ICLK/1=48, PCLKA/1=48, PCLKB/2=24, PCLKC/1=48, PCLKD/1=48, FCLK/2=24.
    //    (divisor encoding: _000=/1, _001=/2). Result: SCKDIVCR = 0x1000_0100.
    system.sckdivcr.write(|w| {
        w.ick()
            ._000()
            .pcka()
            ._000()
            .pckb()
            ._001()
            .pckc()
            ._000()
            .pckd()
            ._000()
            .fck()
            ._001()
    });

    // 6. Switch the system clock source to HOCO (CKSEL=0b000).
    system.sckscr.modify(|_, w| w.cksel()._000());

    // 7. Relock.
    system.prcr.write(|w| unsafe { w.bits(PRCR_LOCK) });

    Clocks {
        sysclk: HOCO_FREQ,
        pclka: HOCO_FREQ,
        pclkb: HOCO_FREQ / 2,
        pclkc: HOCO_FREQ,
        pclkd: HOCO_FREQ,
        fclk: HOCO_FREQ / 2,
    }
}
