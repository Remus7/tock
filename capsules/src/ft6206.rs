//! Driver for the FT6202 Touch Panel.
//!
//! I2C Interface
//!
//! <http://www.tvielectronics.com/ocart/download/controller/FT6206.pdf>
//!
//! The syscall interface is described in [lsm303dlhc.md](https://github.com/tock/tock/tree/master/doc/syscalls/70006_lsm303dlhc.md)
//!
//! Usage
//! -----
//!
//! ```rust
//! let mux_i2c = components::i2c::I2CMuxComponent::new(&stm32f4xx::i2c::I2C1)
//!     .finalize(components::i2c_mux_component_helper!());
//!
//! let ft6206 = components::ft6206::Ft6206Component::new(
//!     stm32f412g::gpio::PinId::PG05.get_pin().as_ref().unwrap(),
//! )
//! .finalize(components::ft6206_i2c_component_helper!(mux_i2c));
//!
//! Author: Alexandru Radovici <msg4alex@gmail.com>

#![allow(non_camel_case_types)]

use core::cell::Cell;
use enum_primitive::cast::FromPrimitive;
use enum_primitive::enum_from_primitive;
use kernel::common::cells::{OptionalCell, TakeCell};
use kernel::debug;
use kernel::hil::gpio;
use kernel::hil::i2c::{self, Error};
use kernel::hil::touch::{self, TouchEvent, TouchStatus, GestureEvent};
use kernel::{AppId, Driver, ReturnCode};

use crate::driver;

/// Syscall driver number.
pub const DRIVER_NUM: usize = driver::NUM::Ft6206 as usize;

// Buffer to use for I2C messages
pub static mut BUFFER: [u8; 17] = [0; 17];

enum State {
    Idle,
    ReadingTouches,
}

enum_from_primitive! {
    enum Registers {
        REG_GEST_ID = 0x01,
        REG_TD_STATUS = 0x02,
        REG_CHIPID = 0xA3,
    }
}

pub struct Ft6206<'a> {
    i2c: &'a dyn i2c::I2CDevice,
    interrupt_pin: &'a dyn gpio::InterruptPin,
    touch_client: OptionalCell<&'static dyn touch::TouchClient>,
    gesture_client: OptionalCell<&'static dyn touch::GestureClient>,
    multi_touch_client: OptionalCell<&'static dyn touch::MultiTouchClient>,
    state: Cell<State>,
    num_touches: Cell<usize>,
    buffer: TakeCell<'static, [u8]>,
}

impl<'a> Ft6206<'a> {
    pub fn new(
        i2c: &'a dyn i2c::I2CDevice,
        interrupt_pin: &'a dyn gpio::InterruptPin,
        buffer: &'static mut [u8],
    ) -> Ft6206<'a> {
        // setup and return struct
        interrupt_pin.enable_interrupts(gpio::InterruptEdge::FallingEdge);
        Ft6206 {
            i2c: i2c,
            interrupt_pin: interrupt_pin,
            touch_client: OptionalCell::empty(),
            gesture_client: OptionalCell::empty(),
            multi_touch_client: OptionalCell::empty(),
            state: Cell::new(State::Idle),
            num_touches: Cell::new(0),
            buffer: TakeCell::new(buffer),
        }
    }

    pub fn is_present(&self) {
        self.state.set(State::Idle);
        self.buffer.take().map(|buf| {
            // turn on i2c to send commands
            buf[0] = 0x92;
            buf[1] = 250;
            self.i2c.write(buf, 2);
        });
    }
}

impl i2c::I2CClient for Ft6206<'_> {
    fn command_complete(&self, buffer: &'static mut [u8], _error: Error) {
        self.state.set(State::Idle);
        self.num_touches.set((buffer[1] & 0x0F) as usize);
        self.touch_client.map(|client| {
            if self.num_touches.get() <= 2 {
                let status = match buffer[1] >> 6 {
                    0x00 => TouchStatus::Pressed,
                    0x01 => TouchStatus::Released,
                    _ => TouchStatus::Released,
                };
                let x = (((buffer[2] & 0x0F) as usize) << 8) + (buffer[3] as usize);
                let y = (((buffer[4] & 0x0F) as usize) << 8) + (buffer[5] as usize);
                let weight = Some(buffer[6] as usize);
                let area = Some(buffer[7] as usize);
                client.touch_event(TouchEvent {
                    status,
                    x,
                    y,
                    id: 0,
                    weight,
                    area,
                });
            }
        });
        self.gesture_client.map(|client| {
            if self.num_touches.get() <= 2 {
                let gesture_event = match buffer[0] {
                    0x10 => Some(GestureEvent::MoveUp),
                    0x14 => Some(GestureEvent::MoveRight),
                    0x18 => Some(GestureEvent::MoveDown),
                    0x1C => Some(GestureEvent::MoveLeft),
                    0x48 => Some(GestureEvent::ZoomIn),
                    0x49 => Some(GestureEvent::ZoomOut),
                    _ => None
                };
                debug! ("{}", buffer[0]);
                if let Some(gesture) = gesture_event {
                    client.gesture_event(gesture);
                }
            }
        });
        // put tyhe buffer back before the multi touch client might ask for events
        self.buffer.replace(buffer);
        self.multi_touch_client.map(|client| {
            if self.num_touches.get() <= 2 {
                client.touch_event(self.num_touches.get ());
            }
        });
        self.interrupt_pin
            .enable_interrupts(gpio::InterruptEdge::FallingEdge);
    }
}

impl gpio::Client for Ft6206<'_> {
    fn fired(&self) {
        self.buffer.take().map(|buffer| {
            self.interrupt_pin.disable_interrupts();

            self.state.set(State::ReadingTouches);

            buffer[0] = Registers::REG_GEST_ID as u8;
            self.i2c.write_read(buffer, 1, 15);
        });
    }
}

impl touch::Touch for Ft6206<'_> {
    fn enable(&self) -> ReturnCode {
        ReturnCode::SUCCESS
    }

    fn disable(&self) -> ReturnCode {
        ReturnCode::SUCCESS
    }

    fn set_client(&self, client: &'static dyn touch::TouchClient) {
        self.touch_client.replace(client);
    }
}

impl touch::Gesture for Ft6206<'_> {
    fn set_client(&self, client: &'static dyn touch::GestureClient) {
        self.gesture_client.replace(client);
    }
}

impl touch::MultiTouch for Ft6206<'_> {
    fn enable(&self) -> ReturnCode {
        ReturnCode::SUCCESS
    }

    fn disable(&self) -> ReturnCode {
        ReturnCode::SUCCESS
    }

    fn get_num_touches(&self) -> usize {
        2
    }

    fn get_touch(&self, index: usize) -> Option<TouchEvent> {
        self.buffer.map_or(None, |buffer| {
            if index <= self.num_touches.get() {
                // a touch has 7 bytes
                let offset = index * 7;
                let status = match buffer[offset + 1] >> 6 {
                    0x00 => TouchStatus::Pressed,
                    0x01 => TouchStatus::Released,
                    _ => TouchStatus::Released,
                };
                let x =
                    (((buffer[offset + 2] & 0x0F) as usize) << 8) + (buffer[offset + 3] as usize);
                let y =
                    (((buffer[offset + 4] & 0x0F) as usize) << 8) + (buffer[offset + 5] as usize);
                let weight = Some(buffer[offset + 6] as usize);
                let area = Some(buffer[offset + 7] as usize);
                Some(TouchEvent {
                    status,
                    x,
                    y,
                    id: 0,
                    weight,
                    area,
                })
            } else {
                None
            }
        })
    }

    fn set_client(&self, client: &'static dyn touch::MultiTouchClient) {
        self.multi_touch_client.replace(client);
    }
}

impl Driver for Ft6206<'_> {
    fn command(&self, command_num: usize, _: usize, _: usize, _: AppId) -> ReturnCode {
        match command_num {
            // is driver present
            0 => ReturnCode::SUCCESS,

            // on
            1 => {
                self.is_present();
                ReturnCode::SUCCESS
            }

            // default
            _ => ReturnCode::ENOSUPPORT,
        }
    }
}
