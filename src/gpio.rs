use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::{
    ErrorType, InputPin, OutputPin, PinState as HalPinState, StatefulOutputPin,
};

const PORT_BASE: usize = 0x4004_0000;
const PORT_OFFSET: usize = 0x20;

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
    fn port_address(&self) -> usize {
        let port_num = match PORT {
            '0'..='9' => (PORT as usize) - ('0' as usize),
            _ => panic!("Invalid port"),
        };
        PORT_BASE + (port_num * PORT_OFFSET)
    }

    fn pin_mask(&self) -> u16 {
        1 << PIN
    }

    unsafe fn read_register(&self, offset: usize) -> u16 {
        let addr = (self.port_address() + offset) as *const u16;
        core::ptr::read_volatile(addr)
    }

    unsafe fn write_register(&self, offset: usize, value: u16) {
        let addr = (self.port_address() + offset) as *mut u16;
        core::ptr::write_volatile(addr, value);
    }

    unsafe fn modify_register(&self, offset: usize, clear_mask: u16, set_mask: u16) {
        let current = self.read_register(offset);
        let new_value = (current & !clear_mask) | set_mask;
        self.write_register(offset, new_value);
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, Input> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        unsafe {
            pin.modify_register(0x00, pin.pin_mask(), 0);
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
        unsafe {
            let value = self.read_register(0x08);
            if value & self.pin_mask() != 0 {
                PinState::High
            } else {
                PinState::Low
            }
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
        unsafe {
            pin.modify_register(0x00, pin.pin_mask(), 0);
            pin.modify_register(0x10, 0, pin.pin_mask());
        }
        pin
    }

    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        unsafe {
            self.modify_register(0x10, self.pin_mask(), 0);
        }
        Pin::<PORT, PIN, Output>::new()
    }

    pub fn into_input(self) -> Pin<PORT, PIN, Input> {
        unsafe {
            self.modify_register(0x10, self.pin_mask(), 0);
        }
        Pin::<PORT, PIN, Input>::new()
    }

    pub fn read(&self) -> PinState {
        unsafe {
            let value = self.read_register(0x08);
            if value & self.pin_mask() != 0 {
                PinState::High
            } else {
                PinState::Low
            }
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
        unsafe {
            pin.modify_register(0x00, 0, pin.pin_mask());
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
        unsafe {
            self.modify_register(0x04, 0, self.pin_mask());
        }
    }

    pub fn set_low(&mut self) {
        unsafe {
            self.modify_register(0x04, self.pin_mask(), 0);
        }
    }

    pub fn set_state(&mut self, state: PinState) {
        match state {
            PinState::Low => self.set_low(),
            PinState::High => self.set_high(),
        }
    }

    pub fn toggle(&mut self) {
        unsafe {
            let current = self.read_register(0x04);
            self.write_register(0x04, current ^ self.pin_mask());
        }
    }

    pub fn is_set_high(&self) -> bool {
        unsafe {
            let value = self.read_register(0x04);
            value & self.pin_mask() != 0
        }
    }

    pub fn is_set_low(&self) -> bool {
        !self.is_set_high()
    }
}

impl<const PORT: char, const PIN: u8> Pin<PORT, PIN, OutputOpenDrain> {
    pub fn new() -> Self {
        let pin = Self { _mode: PhantomData };
        unsafe {
            pin.modify_register(0x00, 0, pin.pin_mask());
            pin.modify_register(0x0C, 0, pin.pin_mask());
        }
        pin
    }

    pub fn into_output(self) -> Pin<PORT, PIN, Output> {
        unsafe {
            self.modify_register(0x0C, self.pin_mask(), 0);
        }
        Pin::<PORT, PIN, Output>::new()
    }

    pub fn set_high(&mut self) {
        unsafe {
            self.modify_register(0x04, 0, self.pin_mask());
        }
    }

    pub fn set_low(&mut self) {
        unsafe {
            self.modify_register(0x04, self.pin_mask(), 0);
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
