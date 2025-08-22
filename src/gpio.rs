use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{
    ErrorType, InputPin, OutputPin, PinState as HalPinState, StatefulOutputPin,
};
use ra4m1::port0::RegisterBlock;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PinMode {
    Input,
    InputPullUp,
    Output,
    OutputOpenDrain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
        match state {
            PinState::Low => false,
            PinState::High => true,
        }
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

#[derive(Debug)]
pub struct Pin<const PORT: char, const PIN: u8, MODE> {
    _mode: PhantomData<MODE>,
}

pub struct Input;
pub struct Output;
pub struct InputPullUp;
pub struct OutputOpenDrain;

impl<const PORT: char, const PIN: u8, MODE> Pin<PORT, PIN, MODE> {
    fn port(&self) -> &RegisterBlock {
        unsafe {
            match PORT {
                '0' => &*(ra4m1::PORT0::PTR as *const RegisterBlock),
                '1' => &*(ra4m1::PORT1::PTR as *const RegisterBlock),
                '2' => &*(ra4m1::PORT2::PTR as *const RegisterBlock),
                '3' => &*(ra4m1::PORT3::PTR as *const RegisterBlock),
                '4' => &*(ra4m1::PORT4::PTR as *const RegisterBlock),
                '5' => &*(ra4m1::PORT5::PTR as *const RegisterBlock),
                '6' => &*(ra4m1::PORT6::PTR as *const RegisterBlock),
                '7' => &*(ra4m1::PORT7::PTR as *const RegisterBlock),
                '8' => &*(ra4m1::PORT8::PTR as *const RegisterBlock),
                '9' => &*(ra4m1::PORT9::PTR as *const RegisterBlock),
                _ => panic!("Invalid port"),
            }
        }
    }

    fn pin_mask(&self) -> u16 {
        1 << PIN
    }

    fn pin_mask_u32(&self) -> u32 {
        1u32 << PIN
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, Input> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        let port = pin.port();
        let mask = pin.pin_mask();

        unsafe {
            port.pdr().modify(|r, w| w.bits(r.bits() & !mask));
        }
        pin
    }

    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        Pin::<PORT, PIN, Output>::new()
    }

    pub fn into_input_pullup(self) -> Pin<PORT, PIN, InputPullUp> {
        Pin::<PORT, PIN, InputPullUp>::new()
    }

    pub fn read(&self) -> PinState {
        let port = self.port();
        let value = port.pidr().read().bits();

        if value & self.pin_mask() != 0 {
            PinState::High
        } else {
            PinState::Low
        }
    }

    pub fn is_high(&self) -> bool {
        self.read() == PinState::High
    }

    pub fn is_low(&self) -> bool {
        self.read() == PinState::Low
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, InputPullUp> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        let port = pin.port();
        let mask = pin.pin_mask();
        let mask_u32 = pin.pin_mask_u32();

        unsafe {
            port.pdr().modify(|r, w| w.bits(r.bits() & !mask));

            port.pcntr1().modify(|r, w| {
                let current = r.bits();
                let pull_up_mask = mask_u32 << 4;
                w.bits(current | pull_up_mask)
            });
        }
        pin
    }

    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        let port = self.port();
        let mask_u32 = self.pin_mask_u32();
        unsafe {
            port.pcntr1().modify(|r, w| {
                let current = r.bits();
                let pull_up_mask = mask_u32 << 4;
                w.bits(current & !pull_up_mask)
            });
        }
        Pin::<PORT, PIN, Output>::new()
    }

    pub fn into_input(self) -> Pin<PORT, PIN, Input> {
        let port = self.port();
        let mask_u32 = self.pin_mask_u32();
        unsafe {
            port.pcntr1().modify(|r, w| {
                let current = r.bits();
                let pull_up_mask = mask_u32 << 4;
                w.bits(current & !pull_up_mask)
            });
        }
        Pin::<PORT, PIN, Input>::new()
    }

    pub fn read(&self) -> PinState {
        let port = self.port();
        let value = port.pidr().read().bits();

        if value & self.pin_mask() != 0 {
            PinState::High
        } else {
            PinState::Low
        }
    }

    pub fn is_high(&self) -> bool {
        self.read() == PinState::High
    }

    pub fn is_low(&self) -> bool {
        self.read() == PinState::Low
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, Output> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        let port = pin.port();
        let mask = pin.pin_mask();

        unsafe {
            port.pdr().modify(|r, w| w.bits(r.bits() | mask));
        }
        pin
    }

    pub fn into_input(self) -> Pin<PORT, PIN, Input> {
        Pin::<PORT, PIN, Input>::new()
    }

    pub fn into_output_open_drain(self) -> Pin<PORT, PIN, OutputOpenDrain> {
        Pin::<PORT, PIN, OutputOpenDrain>::new()
    }

    pub fn set_high(&mut self) {
        let port = self.port();
        let mask = self.pin_mask();
        unsafe {
            port.podr().modify(|r, w| w.bits(r.bits() | mask));
        }
    }

    pub fn set_low(&mut self) {
        let port = self.port();
        let mask = self.pin_mask();
        unsafe {
            port.podr().modify(|r, w| w.bits(r.bits() & !mask));
        }
    }

    pub fn set_state(&mut self, state: PinState) {
        match state {
            PinState::Low => self.set_low(),
            PinState::High => self.set_high(),
        }
    }

    pub fn toggle(&mut self) {
        let port = self.port();
        let mask = self.pin_mask();
        unsafe {
            port.podr().modify(|r, w| w.bits(r.bits() ^ mask));
        }
    }

    pub fn is_set_high(&self) -> bool {
        let port = self.port();
        let value = port.podr().read().bits();
        value & self.pin_mask() != 0
    }

    pub fn is_set_low(&self) -> bool {
        !self.is_set_high()
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, OutputOpenDrain> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        let port = pin.port();
        let mask = pin.pin_mask();
        let mask_u32 = pin.pin_mask_u32();

        unsafe {
            port.pdr().modify(|r, w| w.bits(r.bits() | mask));

            port.pcntr1().modify(|r, w| {
                let current = r.bits();
                let ndr_mask = mask_u32 << 8;
                w.bits(current | ndr_mask)
            });
        }
        pin
    }

    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        let port = self.port();
        let mask_u32 = self.pin_mask_u32();
        unsafe {
            port.pcntr1().modify(|r, w| {
                let current = r.bits();
                let ndr_mask = mask_u32 << 8;
                w.bits(current & !ndr_mask)
            });
        }
        Pin::<PORT, PIN, Output>::new()
    }

    pub fn set_high(&mut self) {
        let port = self.port();
        let mask = self.pin_mask();
        unsafe {
            port.podr().modify(|r, w| w.bits(r.bits() | mask));
        }
    }

    pub fn set_low(&mut self) {
        let port = self.port();
        let mask = self.pin_mask();
        unsafe {
            port.podr().modify(|r, w| w.bits(r.bits() & !mask));
        }
    }

    pub fn set_state(&mut self, state: PinState) {
        match state {
            PinState::Low => self.set_low(),
            PinState::High => self.set_high(),
        }
    }
}

impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, Input> {
    type Error = Infallible;
}

impl<const PORT: char, const PIN: u8> InputPin for Pin<PORT, PIN, Input> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, Input>::is_high(self))
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, Input>::is_low(self))
    }
}

impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, InputPullUp> {
    type Error = Infallible;
}

impl<const PORT: char, const PIN: u8> InputPin for Pin<PORT, PIN, InputPullUp> {
    fn is_high(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, InputPullUp>::is_high(self))
    }

    fn is_low(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, InputPullUp>::is_low(self))
    }
}

impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, Output> {
    type Error = Infallible;
}

impl<const PORT: char, const PIN: u8> OutputPin for Pin<PORT, PIN, Output> {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.set_high();
        Ok(())
    }
}

impl<const PORT: char, const PIN: u8> StatefulOutputPin for Pin<PORT, PIN, Output> {
    fn is_set_high(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, Output>::is_set_high(self))
    }

    fn is_set_low(&mut self) -> Result<bool, Self::Error> {
        Ok(Pin::<PORT, PIN, Output>::is_set_low(self))
    }

    fn toggle(&mut self) -> Result<(), Self::Error> {
        Pin::<PORT, PIN, Output>::toggle(self);
        Ok(())
    }
}

impl<const PORT: char, const PIN: u8> ErrorType for Pin<PORT, PIN, OutputOpenDrain> {
    type Error = Infallible;
}

impl<const PORT: char, const PIN: u8> OutputPin for Pin<PORT, PIN, OutputOpenDrain> {
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.set_low();
        Ok(())
    }

    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.set_high();
        Ok(())
    }
}

pub type P0_0 = Pin<'0', 0, Input>;
pub type P0_1 = Pin<'0', 1, Input>;
pub type P0_2 = Pin<'0', 2, Input>;
pub type P1_0 = Pin<'1', 0, Input>;
pub type P1_1 = Pin<'1', 1, Input>;
pub type P1_2 = Pin<'1', 2, Input>;
pub type P1_11 = Pin<'1', 11, Input>;
pub type P1_12 = Pin<'1', 12, Input>;
pub type P3_1 = Pin<'3', 1, Input>;
pub type P3_2 = Pin<'3', 2, Input>;
