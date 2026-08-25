//! General-purpose I/O (GPIO).
//!
//! Pins are represented as the const-generic typestate struct [`Pin`]. `PORT` is the port
//! letter (`'0'`..`'9'`), `PIN` is the pin number within the port (0..15).
//!
//! # Registers (RA4M1)
//!
//! - Direction, pull-up, open-drain, and peripheral/analog assignment are all set through
//!   the per-pin `PmnPFS` register. `PmnPFS` requires unlocking `PWPR` first, so mode
//!   changes go through `modify_pfs` (unlock, edit, relock inside a critical section).
//! - Output set/clear at runtime uses the port's `POSR`/`PORR` (write-1-to-set/reset,
//!   atomic) to avoid read-modify-write races. Input reads use `PIDR`.
//!
//! Each physical pin can only be taken out of [`Pins`] once, guaranteeing exclusive ownership.

use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{
    ErrorType, InputPin, OutputPin, PinState as HalPinState, StatefulOutputPin,
};
use ra4m1::port0::RegisterBlock;

// ===== Raw PFS / PWPR access =====

/// Base address of the `PmnPFS` register array.
const PFS_BASE: usize = 0x4004_0800;
/// Address of `PWPR` (inside PMISC, offset 0x03). 8-bit register.
const PWPR_ADDR: *mut u8 = 0x4004_0d03 as *mut u8;

// PmnPFS bit masks (positions match the PAC field definitions).
const PFS_PDR: u32 = 1 << 2; // direction (0=input, 1=output)
const PFS_PCR: u32 = 1 << 4; // input pull-up
const PFS_NCODR: u32 = 1 << 6; // N-ch open drain
const PFS_ASEL: u32 = 1 << 15; // analog input assignment
const PFS_PMR: u32 = 1 << 16; // port mode (0=GPIO, 1=peripheral)
const PFS_PSEL_MASK: u32 = 0x1F << 24; // peripheral select [28:24]

/// Computes the `PmnPFS` register address for `Pin<PORT, PIN, _>`.
#[inline]
fn pfs_addr(port: char, pin: u8) -> *mut u32 {
    let p = (port as u8 - b'0') as usize;
    (PFS_BASE + 0x40 * p + 4 * pin as usize) as *mut u32
}

/// Unlocks `PWPR`, read-modify-writes the pin's `PmnPFS`, then relocks it.
///
/// Wrapped in a critical section so the unlocked window can't overlap another pin's
/// configuration.
#[inline]
fn modify_pfs(port: char, pin: u8, clear: u32, set: u32) {
    critical_section::with(|_| unsafe {
        // Unlock: B0WI=0 first (makes PFSWE writable), then PFSWE=1.
        core::ptr::write_volatile(PWPR_ADDR, 0x00);
        core::ptr::write_volatile(PWPR_ADDR, 0x40);

        let addr = pfs_addr(port, pin);
        let v = core::ptr::read_volatile(addr);
        core::ptr::write_volatile(addr, (v & !clear) | set);

        // Relock: PFSWE=0, then B0WI=1.
        core::ptr::write_volatile(PWPR_ADDR, 0x00);
        core::ptr::write_volatile(PWPR_ADDR, 0x80);
    });
}

// ===== Public types =====

/// Logical pin state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinState {
    Low,
    High,
}

impl From<bool> for PinState {
    fn from(value: bool) -> Self {
        if value {
            PinState::High
        } else {
            PinState::Low
        }
    }
}

impl From<PinState> for bool {
    fn from(state: PinState) -> Self {
        matches!(state, PinState::High)
    }
}

impl From<HalPinState> for PinState {
    fn from(state: HalPinState) -> Self {
        match state {
            HalPinState::Low => PinState::Low,
            HalPinState::High => PinState::High,
        }
    }
}

// Zero-sized typestate markers for pin mode.
/// Input (no pull-up).
pub struct Input;
/// Input (pull-up enabled).
pub struct InputPullUp;
/// Output (CMOS push-pull).
pub struct Output;
/// Output (N-ch open drain).
pub struct OutputOpenDrain;
/// Analog input (used with the ADC).
pub struct Analog;
/// Peripheral function assignment (controlled by a UART/PWM driver, etc.).
pub struct Alternate;

/// Typestate GPIO pin. `PORT` is the port letter, `PIN` is the pin number.
#[derive(Debug)]
pub struct Pin<const PORT: char, const PIN: u8, MODE> {
    _mode: PhantomData<MODE>,
}

impl<const PORT: char, const PIN: u8, MODE> Pin<PORT, PIN, MODE> {
    /// Creates a new pin token (touches no hardware).
    /// Only called from within the crate ([`Pins`] / drivers) to preserve exclusive ownership.
    pub(crate) const fn new() -> Self {
        Self { _mode: PhantomData }
    }

    /// Register block of the port this pin belongs to.
    ///
    /// All ports share the same layout and are spaced 0x20 apart starting at `PORT0`
    /// (0x4004_0000). The registers this HAL uses (POSR/PORR, PIDR, ...) all live in that
    /// shared layout.
    #[inline]
    fn port(&self) -> &RegisterBlock {
        let index = (PORT as u8 - b'0') as usize;
        unsafe { &*((0x4004_0000usize + 0x20 * index) as *const RegisterBlock) }
    }

    /// Bitmask of this pin within its port.
    #[inline]
    fn mask(&self) -> u16 {
        1u16 << PIN
    }

    // --- Mode transitions (from any mode) ---

    /// Switches to input (no pull-up).
    pub fn into_input(self) -> Pin<PORT, PIN, Input> {
        modify_pfs(
            PORT,
            PIN,
            PFS_PDR | PFS_PCR | PFS_NCODR | PFS_PMR | PFS_ASEL | PFS_PSEL_MASK,
            0,
        );
        Pin::new()
    }

    /// Switches to input with pull-up enabled.
    pub fn into_input_pullup(self) -> Pin<PORT, PIN, InputPullUp> {
        modify_pfs(
            PORT,
            PIN,
            PFS_PDR | PFS_NCODR | PFS_PMR | PFS_ASEL | PFS_PSEL_MASK,
            PFS_PCR,
        );
        Pin::new()
    }

    /// Switches to push-pull output.
    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        self.into_output_in_state(PinState::Low)
    }

    /// Switches to push-pull output with the requested initial state.
    ///
    /// The output latch is set before the output driver is enabled, avoiding a transient
    /// pulse during the mode change.
    pub fn into_output_in_state(self, state: PinState) -> Pin<PORT, PIN, Output> {
        match state {
            PinState::High => unsafe { self.port().posr().write(|w| w.bits(self.mask())) },
            PinState::Low => unsafe { self.port().porr().write(|w| w.bits(self.mask())) },
        }
        modify_pfs(
            PORT,
            PIN,
            PFS_PCR | PFS_NCODR | PFS_PMR | PFS_ASEL | PFS_PSEL_MASK,
            PFS_PDR,
        );
        Pin::new()
    }

    /// Switches to open-drain output.
    pub fn into_output_open_drain(self) -> Pin<PORT, PIN, OutputOpenDrain> {
        // Released is the safest default for an open-drain bus.
        self.into_output_open_drain_in_state(PinState::High)
    }

    /// Switches to open-drain output with the requested initial state.
    ///
    /// [`PinState::High`] releases the line and [`PinState::Low`] actively pulls it low.
    pub fn into_output_open_drain_in_state(
        self,
        state: PinState,
    ) -> Pin<PORT, PIN, OutputOpenDrain> {
        match state {
            PinState::High => unsafe { self.port().posr().write(|w| w.bits(self.mask())) },
            PinState::Low => unsafe { self.port().porr().write(|w| w.bits(self.mask())) },
        }
        modify_pfs(
            PORT,
            PIN,
            PFS_PCR | PFS_PMR | PFS_ASEL | PFS_PSEL_MASK,
            PFS_PDR | PFS_NCODR,
        );
        Pin::new()
    }

    /// Switches to analog input (ASEL=1, GPIO output stage disabled).
    pub fn into_analog(self) -> Pin<PORT, PIN, Analog> {
        modify_pfs(
            PORT,
            PIN,
            PFS_PDR | PFS_PCR | PFS_NCODR | PFS_PMR | PFS_PSEL_MASK,
            PFS_ASEL,
        );
        Pin::new()
    }

    /// Assigns the pin to a peripheral function (PMR=1, PSEL=`psel`). Used by UART/PWM drivers.
    ///
    /// Sets PSEL first (while PMR=0), then sets PMR=1.
    pub(crate) fn into_alternate(self, psel: u8) -> Pin<PORT, PIN, Alternate> {
        modify_pfs(
            PORT,
            PIN,
            PFS_PCR | PFS_NCODR | PFS_ASEL | PFS_PMR | PFS_PSEL_MASK,
            (psel as u32 & 0x1F) << 24,
        );
        modify_pfs(PORT, PIN, 0, PFS_PMR);
        Pin::new()
    }
}

// --- Input methods ---

macro_rules! impl_input {
    ($mode:ty) => {
        impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, $mode> {
            /// Reads the input level.
            #[inline]
            pub fn read(&self) -> PinState {
                let high = (self.port().pidr().read().bits() & self.mask()) != 0;
                PinState::from(high)
            }
            /// Whether the pin is high.
            #[inline]
            pub fn is_high(&self) -> bool {
                self.read() == PinState::High
            }
            /// Whether the pin is low.
            #[inline]
            pub fn is_low(&self) -> bool {
                self.read() == PinState::Low
            }
        }

        impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, $mode> {
            type Error = Infallible;
        }

        impl<const PORT: char, const PIN: u8> InputPin for Pin<PORT, PIN, $mode> {
            fn is_high(&mut self) -> Result<bool, Self::Error> {
                Ok(Pin::<PORT, PIN, $mode>::is_high(self))
            }
            fn is_low(&mut self) -> Result<bool, Self::Error> {
                Ok(Pin::<PORT, PIN, $mode>::is_low(self))
            }
        }
    };
}

impl_input!(Input);
impl_input!(InputPullUp);

// --- Output methods ---

macro_rules! impl_output {
    ($mode:ty) => {
        impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, $mode> {
            /// Drives the output high (atomic write to POSR).
            #[inline]
            pub fn set_high(&mut self) {
                unsafe { self.port().posr().write(|w| w.bits(self.mask())) };
            }
            /// Drives the output low (atomic write to PORR).
            #[inline]
            pub fn set_low(&mut self) {
                unsafe { self.port().porr().write(|w| w.bits(self.mask())) };
            }
            /// Drives the output to the given state.
            #[inline]
            pub fn set_state(&mut self, state: PinState) {
                match state {
                    PinState::High => self.set_high(),
                    PinState::Low => self.set_low(),
                }
            }
            /// Whether the output latch is currently high (reads PODR).
            #[inline]
            pub fn is_set_high(&self) -> bool {
                (self.port().podr().read().bits() & self.mask()) != 0
            }
            /// Whether the output latch is currently low.
            #[inline]
            pub fn is_set_low(&self) -> bool {
                (self.port().podr().read().bits() & self.mask()) == 0
            }
            /// Toggles the output.
            #[inline]
            pub fn toggle(&mut self) {
                let mask = self.mask();
                let is_high = (self.port().podr().read().bits() & mask) != 0;
                if is_high {
                    unsafe { self.port().porr().write(|w| w.bits(mask)) };
                } else {
                    unsafe { self.port().posr().write(|w| w.bits(mask)) };
                }
            }
        }

        impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, $mode> {
            type Error = Infallible;
        }

        impl<const PORT: char, const PIN: u8> OutputPin for Pin<PORT, PIN, $mode> {
            fn set_low(&mut self) -> Result<(), Self::Error> {
                Pin::<PORT, PIN, $mode>::set_low(self);
                Ok(())
            }
            fn set_high(&mut self) -> Result<(), Self::Error> {
                Pin::<PORT, PIN, $mode>::set_high(self);
                Ok(())
            }
        }

        impl<const PORT: char, const PIN: u8> StatefulOutputPin for Pin<PORT, PIN, $mode> {
            fn is_set_high(&mut self) -> Result<bool, Self::Error> {
                Ok(Pin::<PORT, PIN, $mode>::is_set_high(self))
            }
            fn is_set_low(&mut self) -> Result<bool, Self::Error> {
                Ok(Pin::<PORT, PIN, $mode>::is_set_low(self))
            }
            fn toggle(&mut self) -> Result<(), Self::Error> {
                Pin::<PORT, PIN, $mode>::toggle(self);
                Ok(())
            }
        }
    };
}

impl_output!(Output);
impl_output!(OutputOpenDrain);

// ===== Ownership gate: Pins =====

/// GPIO pins corresponding to the Arduino Uno R4 header.
///
/// Built by [`Pins::new`], which consumes the PAC's port/PFS/PMISC peripherals (normally
/// done internally by [`crate::Peripherals::take`]). Each field starts in the reset-default
/// input mode; use `into_output()` etc. to change it.
///
/// `d13` is wired to the onboard LED (LED_BUILTIN).
#[allow(missing_docs)]
pub struct Pins {
    pub d0: Pin<'3', 1, Input>, // RX (Serial1)
    pub d1: Pin<'3', 2, Input>, // TX (Serial1)
    #[cfg(feature = "uno-r4-minima")]
    pub d2: Pin<'1', 5, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d2: Pin<'1', 4, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d3: Pin<'1', 4, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d3: Pin<'1', 5, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d4: Pin<'1', 3, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d4: Pin<'1', 6, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d5: Pin<'1', 2, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d5: Pin<'1', 7, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d6: Pin<'1', 6, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d6: Pin<'1', 11, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d7: Pin<'1', 7, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d7: Pin<'1', 12, Input>,
    pub d8: Pin<'3', 4, Input>,
    pub d9: Pin<'3', 3, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d10: Pin<'1', 12, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d10: Pin<'1', 3, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d11: Pin<'1', 9, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d11: Pin<'4', 11, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d12: Pin<'1', 10, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d12: Pin<'4', 10, Input>,
    #[cfg(feature = "uno-r4-minima")]
    pub d13: Pin<'1', 11, Input>,
    #[cfg(feature = "uno-r4-wifi")]
    pub d13: Pin<'1', 2, Input>, // LED_BUILTIN / SCK
    pub a0: Pin<'0', 14, Input>,
    pub a1: Pin<'0', 0, Input>,
    pub a2: Pin<'0', 1, Input>,
    pub a3: Pin<'0', 2, Input>,
    pub a4: Pin<'1', 1, Input>, // SDA
    pub a5: Pin<'1', 0, Input>, // SCL
}

impl Pins {
    /// Consumes the GPIO-related peripherals and builds the pin set.
    ///
    /// The peripheral arguments are taken only to guarantee exclusive ownership; actual
    /// register access happens through each pin's computed address.
    pub fn new(
        _port0: ra4m1::PORT0,
        _port1: ra4m1::PORT1,
        _port3: ra4m1::PORT3,
        _port4: ra4m1::PORT4,
        _pfs: ra4m1::PFS,
        _pmisc: ra4m1::PMISC,
    ) -> Self {
        Self {
            d0: Pin::new(),
            d1: Pin::new(),
            d2: Pin::new(),
            d3: Pin::new(),
            d4: Pin::new(),
            d5: Pin::new(),
            d6: Pin::new(),
            d7: Pin::new(),
            d8: Pin::new(),
            d9: Pin::new(),
            d10: Pin::new(),
            d11: Pin::new(),
            d12: Pin::new(),
            d13: Pin::new(),
            a0: Pin::new(),
            a1: Pin::new(),
            a2: Pin::new(),
            a3: Pin::new(),
            a4: Pin::new(),
            a5: Pin::new(),
        }
    }
}
