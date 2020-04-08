//! Components for I2C.
//!
//! This provides three components.
//!
//! 1. `I2CMuxComponent` provides a virtualization layer for a I2C bus.
//!
//! 2. `I2CComponent` provides a virtualized client to the I2C bus.
//!
//! Usage
//! -----
//! ```rust
//! let mux_i2c = components::i2c::I2CMuxComponent::new(&stm32f3xx::i2c::I2C1).finalize(());
//! let client_i2c = components::i2c::I2CComponent::new(mux_i2c, address).finalize(());
//! ```

// Author: Alexandru Radovici <msg4alex@gmail.com>

use capsules::virtual_i2c::{I2CDevice, MuxI2C};
use kernel::component::Component;
use kernel::hil::i2c;
use kernel::static_init;

pub struct I2CMuxComponent {
    i2c: &'static dyn i2c::I2CMaster,
}

pub struct I2CComponent {
    i2c_mux: &'static MuxI2C<'static>,
    address: u8,
}

impl I2CMuxComponent {
    pub fn new(i2c: &'static dyn i2c::I2CMaster) -> Self {
        I2CMuxComponent { i2c: i2c }
    }
}

impl Component for I2CMuxComponent {
    type StaticInput = ();
    type Output = &'static MuxI2C<'static>;

    unsafe fn finalize(self, _static_buffer: Self::StaticInput) -> Self::Output {
        let mux_i2c = static_init!(MuxI2C<'static>, MuxI2C::new(self.i2c));

        self.i2c.set_master_client(mux_i2c);

        mux_i2c
    }
}

impl I2CComponent {
    pub fn new(mux: &'static MuxI2C<'static>, address: u8) -> Self {
        I2CComponent {
            i2c_mux: mux,
            address: address,
        }
    }
}

impl Component for I2CComponent {
    type StaticInput = ();
    type Output = &'static I2CDevice<'static>;

    unsafe fn finalize(self, _static_buffer: Self::StaticInput) -> Self::Output {
        let i2c_device = static_init!(
            I2CDevice<'static>,
            I2CDevice::new(self.i2c_mux, self.address)
        );

        i2c_device
    }
}
