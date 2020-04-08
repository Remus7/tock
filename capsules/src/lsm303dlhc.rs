//! Driver for the LSM303DLHC 3D accelerometer and 3D magnetometer sensor.
//!
//! <https://www.st.com/en/mems-and-sensors/lsm303dlhc.html>
//!
//! Usage
//! -----
//!
//! ```rust
//! let lps25hb_i2c = static_init!(I2CDevice, I2CDevice::new(i2c_bus, 0x5C));
//! let lps25hb = static_init!(
//!     capsules::lps25hb::LPS25HB<'static>,
//!     capsules::lps25hb::LPS25HB::new(lps25hb_i2c,
//!         &sam4l::gpio::PA[10],
//!         &mut capsules::lps25hb::BUFFER));
//! lps25hb_i2c.set_client(lps25hb);
//! sam4l::gpio::PA[10].set_client(lps25hb);
//! ```

use core::cell::Cell;
use enum_primitive::cast::FromPrimitive;
use enum_primitive::cast::ToPrimitive;
use enum_primitive::enum_from_primitive;
use kernel::common::cells::{OptionalCell, TakeCell};
use kernel::debug;
use kernel::hil::gpio;
use kernel::hil::i2c::{self, Error};
use kernel::{AppId, Callback, Driver, ReturnCode};

/// Syscall driver number.
use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::Lsm303dlch as usize;

// Buffer to use for I2C messages
pub static mut BUFFER: [u8; 5] = [0; 5];

/// Register values
const REGISTER_AUTO_INCREMENT: u8 = 0x80;

/// Accelerometer Registers
const CTRL_REG1: u8 = 0x20;
const CTRL_REG4: u8 = 0x23;
const OUT_X_L_A: u8 = 0x28;
const OUT_X_H_A: u8 = 0x29;
const OUT_Y_L_A: u8 = 0x2A;
const OUT_Y_H_A: u8 = 0x2B;
const OUT_Z_L_A: u8 = 0x2C;
const OUT_Z_H_A: u8 = 0x2D;

/// Magnetometer Registers
const CRA_REG_M: u8 = 0x00;
const CRB_REG_M: u8 = 0x01;
const OUT_X_H_M: u8 = 0x03;
const OUT_X_L_M: u8 = 0x04;
const OUT_Z_H_M: u8 = 0x05;
const OUT_Z_L_M: u8 = 0x06;
const OUT_Y_H_M: u8 = 0x07;
const OUT_Y_L_M: u8 = 0x08;
const TEMP_OUT_H_M: u8 = 0x31;
const TEMP_OUT_L_M: u8 = 0x32;

enum_from_primitive! {
    #[derive(Clone, Copy, PartialEq)]
    pub enum Lsm303dlhcAccelDataRate {
        Off = 0,
        DataRate1Hz = 1,
        DataRate10Hz = 2,
        DataRate25Hz = 3,
        DataRate50Hz = 4,
        DataRate100Hz = 5,
        DataRate200Hz = 6,
        DataRate400Hz = 7,
        LowPower1620Hz = 8,
        Normal1344LowPower5376Hz = 9,
    }
}

enum_from_primitive! {
    #[derive(Clone, Copy, PartialEq)]
    pub enum Lsm303dlhcMagnetoDataRate {
        DataRate0_75Hz = 0,
        DataRate1_5Hz = 1,
        DataRate3_0Hz = 2,
        DataRate7_5Hz = 3,
        DataRate15_0Hz = 4,
        DataRate30_0Hz = 5,
        DataRate75_0Hz = 6,
        DataRate220_0Hz = 7,
    }
}

enum_from_primitive! {
    #[derive(Clone, Copy, PartialEq)]
    pub enum Lsm303dlhcScale {
        Scale2G = 0,
        Scale4G = 1,
        Scale8G = 2,
        Scale16G = 3
    }
}

enum_from_primitive! {
    #[derive(Clone, Copy, PartialEq)]
    pub enum Lsm303dlhcRange {
        Range1G = 0,
        Range1_3G = 1,
        Range1_9G = 2,
        Range2_5G = 3,
        Range4_0G = 4,
        Range4_7G = 5,
        Range5_6G = 7,
        Range8_1 = 8,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Idle,
    IsPresent,
    SetPowerMode,
    SetScaleAndResolution,
    ReadAccelerationXYZ,
    SetTemperatureDataRate,
    SetRange,
    ReadTemperature,
    ReadMagnetometerXYZ,
}

pub struct Lsm303dlhc<'a> {
    i2c_accelerometer: &'a dyn i2c::I2CDevice,
    i2c_magnetometer: &'a dyn i2c::I2CDevice,
    callback: OptionalCell<Callback>,
    state: Cell<State>,
    scale: Cell<Lsm303dlhcScale>,
    range: Cell<Lsm303dlhcRange>,
    high_resolution: Cell<bool>,
    buffer: TakeCell<'static, [u8]>,
}

impl Lsm303dlhc<'a> {
    pub fn new(
        i2c_accelerometer: &'a dyn i2c::I2CDevice,
        i2c_magnetometer: &'a dyn i2c::I2CDevice,
        buffer: &'static mut [u8],
    ) -> Lsm303dlhc<'a> {
        // setup and return struct
        Lsm303dlhc {
            i2c_accelerometer: i2c_accelerometer,
            i2c_magnetometer: i2c_magnetometer,
            callback: OptionalCell::empty(),
            state: Cell::new(State::Idle),
            scale: Cell::new(Lsm303dlhcScale::Scale2G),
            range: Cell::new(Lsm303dlhcRange::Range1G),
            high_resolution: Cell::new(false),
            buffer: TakeCell::new(buffer),
        }
    }

    fn is_present(&self) {
        self.state.set(State::IsPresent);
        self.buffer.take().map(|buf| {
            // turn on i2c to send commands
            buf[0] = 0x0F;
            self.i2c_magnetometer.write_read(buf, 1, 1);
        });
    }

    pub fn set_power_mode(&self, data_rate: Lsm303dlhcAccelDataRate, low_power: bool) {
        if self.state.get() == State::Idle {
            self.state.set(State::SetPowerMode);
            self.buffer.take().map(|buf| {
                buf[0] = CTRL_REG1;
                buf[1] = ((data_rate as u8) << 4) | if low_power { 1 << 3 } else { 0 } | 0x7;
                self.i2c_accelerometer.write(buf, 2);
            });
        }
    }

    pub fn set_scale_and_resolution(&self, scale: Lsm303dlhcScale, high_resolution: bool) {
        if self.state.get() == State::Idle {
            self.state.set(State::SetScaleAndResolution);
            // TODO move these in completed
            self.scale.set(scale);
            self.high_resolution.set(high_resolution);
            self.buffer.take().map(|buf| {
                buf[0] = CTRL_REG4;
                buf[1] = (scale as u8) << 4 | if high_resolution { 1 } else { 0 } << 3;
                self.i2c_accelerometer.write(buf, 2);
            });
        }
    }

    pub fn read_acceleration_xyz(&self) {
        if self.state.get() == State::Idle {
            self.state.set(State::ReadAccelerationXYZ);
            self.buffer.take().map(|buf| {
                buf[0] = OUT_X_L_A | REGISTER_AUTO_INCREMENT;
                self.i2c_accelerometer.write_read(buf, 1, 6);
            });
        }
    }

    pub fn set_temperature_and_magneto_data_rate(&self, temperature: bool, data_rate: Lsm303dlhcMagnetoDataRate) {
        if self.state.get() == State::Idle {
            self.state.set(State::SetTemperatureDataRate);
            self.buffer.take().map(|buf| {
                buf[0] = CRA_REG_M;
                buf[1] = ((data_rate as u8) << 2) | if temperature { 1 << 7 } else { 0 };
                self.i2c_magnetometer.write(buf, 2);
            });
        }
    }
    
    pub fn set_range(&self, range: Lsm303dlhcRange) {
        if self.state.get() == State::Idle {
            self.state.set(State::SetRange);
            // TODO move these in completed
            self.range.set(range);
            self.buffer.take().map(|buf| {
                buf[0] = CRB_REG_M;
                buf[1] = (range as u8) << 5;
                buf[2] = 0;
                self.i2c_magnetometer.write(buf, 3);
            });
        }
    }

    pub fn read_temperature (&self) {
        if self.state.get() == State::Idle {
            self.state.set(State::ReadAccelerationXYZ);
            self.buffer.take().map(|buf| {
                buf[0] = TEMP_OUT_H_M;
                self.i2c_magnetometer.write_read(buf, 1, 2);
            });
        }
    }

    pub fn read_magnetometer_xyz(&self) {
        if self.state.get() == State::Idle {
            self.state.set(State::ReadMagnetometerXYZ);
            self.buffer.take().map(|buf| {
                buf[0] = OUT_X_H_M;
                self.i2c_magnetometer.write_read(buf, 1, 6);
            });
        }
    }
}

impl i2c::I2CClient for Lsm303dlhc<'a> {
    fn command_complete(&self, buffer: &'static mut [u8], error: Error) {
        match self.state.get() {
            State::IsPresent => {
                let present = if error == Error::CommandComplete && buffer[0] == 60 {
                    true
                } else {
                    false
                };

                self.callback.map(|callback| {
                    callback.schedule(if present { 1 } else { 0 }, 0, 0);
                });
            }
            State::SetPowerMode => {
                let set_power = error == Error::CommandComplete;

                self.callback.map(|callback| {
                    callback.schedule(if set_power { 1 } else { 0 }, 0, 0);
                });
            }
            State::SetScaleAndResolution => {
                let set_scale_and_resolution = error == Error::CommandComplete;

                self.callback.map(|callback| {
                    callback.schedule(if set_scale_and_resolution { 1 } else { 0 }, 0, 0);
                });
            }
            State::ReadAccelerationXYZ => {
                let mut x: usize = 0;
                let mut y: usize = 0;
                let mut z: usize = 0;
                let values = if error == Error::CommandComplete {
                    // self.nine_dof_client.map(|client| {
                    //     // compute using only integers
                    //     let scale = match self.scale.get() {
                    //         0 => L3GD20_SCALE_250,
                    //         1 => L3GD20_SCALE_500,
                    //         _ => L3GD20_SCALE_2000,
                    //     };
                    //     let x: usize = ((buf[1] as i16 | ((buf[2] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     let y: usize = ((buf[3] as i16 | ((buf[4] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     let z: usize = ((buf[5] as i16 | ((buf[6] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     client.callback(x, y, z);
                    // });

                    x = (buffer[0] as i16 | ((buffer[1] as i16) << 8)) as usize;
                    y = (buffer[2] as i16 | ((buffer[3] as i16) << 8)) as usize;
                    z = (buffer[4] as i16 | ((buffer[5] as i16) << 8)) as usize;
                    true
                } else {
                    // self.nine_dof_client.map(|client| {
                    //     client.callback(0, 0, 0);
                    // });
                    false
                };
                if values {
                    self.callback.map(|callback| {
                        callback.schedule(x, y, z);
                    });
                } else {
                    self.callback.map(|callback| {
                        callback.schedule(0, 0, 0);
                    });
                }
            }
            State::SetTemperatureDataRate => {
                let set_temperature_and_magneto_data_rate = error == Error::CommandComplete;

                self.callback.map(|callback| {
                    callback.schedule(if set_temperature_and_magneto_data_rate { 1 } else { 0 }, 0, 0);
                });
            }
            State::SetRange => {
                let set_range = error == Error::CommandComplete;

                self.callback.map(|callback| {
                    callback.schedule(if set_range { 1 } else { 0 }, 0, 0);
                });
            }
            State::ReadTemperature => {
                let mut temp: usize = 0;
                let values = if error == Error::CommandComplete {
                    temp = ((buffer[1] as i16 | ((buffer[0] as i16) << 8)) >> 3) as usize;
                    true
                } else {
                    // self.nine_dof_client.map(|client| {
                    //     client.callback(0, 0, 0);
                    // });
                    false
                };
                if values {
                    self.callback.map(|callback| {
                        callback.schedule(temp, 0, 0);
                    });
                } else {
                    self.callback.map(|callback| {
                        callback.schedule(0, 0, 0);
                    });
                }
            }
            State::ReadMagnetometerXYZ => {
                let mut x: usize = 0;
                let mut y: usize = 0;
                let mut z: usize = 0;
                let values = if error == Error::CommandComplete {
                    // self.nine_dof_client.map(|client| {
                    //     // compute using only integers
                    //     let scale = match self.scale.get() {
                    //         0 => L3GD20_SCALE_250,
                    //         1 => L3GD20_SCALE_500,
                    //         _ => L3GD20_SCALE_2000,
                    //     };
                    //     let x: usize = ((buf[1] as i16 | ((buf[2] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     let y: usize = ((buf[3] as i16 | ((buf[4] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     let z: usize = ((buf[5] as i16 | ((buf[6] as i16) << 8)) as isize * scale
                    //         / 100000) as usize;
                    //     client.callback(x, y, z);
                    // });

                    x = (buffer[1] as i16 | ((buffer[0] as i16) << 8)) as usize;
                    z = (buffer[3] as i16 | ((buffer[2] as i16) << 8)) as usize;
                    y = (buffer[5] as i16 | ((buffer[4] as i16) << 8)) as usize;
                    true
                } else {
                    // self.nine_dof_client.map(|client| {
                    //     client.callback(0, 0, 0);
                    // });
                    false
                };
                if values {
                    self.callback.map(|callback| {
                        callback.schedule(x, y, z);
                    });
                } else {
                    self.callback.map(|callback| {
                        callback.schedule(0, 0, 0);
                    });
                }
            }
            _ => {
                debug!("buffer {:?} error {:?}", buffer, error);
            }
        }
        self.buffer.replace(buffer);
        self.state.set(State::Idle);
    }
}

impl Driver for Lsm303dlhc<'a> {
    fn command(&self, command_num: usize, data1: usize, data2: usize, _: AppId) -> ReturnCode {
        match command_num {
            0 /* check if present */ => ReturnCode::SUCCESS,
            // Check is sensor is correctly connected
            1 => {
				if self.state.get () == State::Idle {
					self.is_present ();
					ReturnCode::SUCCESS
				}
				else
				{
					ReturnCode::EBUSY
				}

			}
			// Set Accelerometer Power Mode
            2 => {
				if self.state.get () == State::Idle {
                    if let Some (data_rate) = Lsm303dlhcAccelDataRate::from_usize (data1) {
                        self.set_power_mode(data_rate, if data2 != 0 { true } else { false });
                        ReturnCode::SUCCESS
                    }
                    else
                    {
                        ReturnCode::EINVAL
                    }
				}
				else
				{
					ReturnCode::EBUSY
				}
			}
			// Set Accelerometer Scale And Resolution
            3 => {
				if self.state.get () == State::Idle {
					if let Some (scale) = Lsm303dlhcScale::from_usize(data1) {
                        self.set_scale_and_resolution(scale, if data2 != 0 { true } else { false });
                        ReturnCode::SUCCESS
                    }
                    else
                    {
                        ReturnCode::EINVAL
                    }
				}
				else
				{
					ReturnCode::EBUSY
				}
            }
            // Set Magnetometer Temperature Enable and Data Rate 
            4 => {
				if self.state.get () == State::SetTemperatureDataRate {
                    if let Some (data_rate) = Lsm303dlhcMagnetoDataRate::from_usize (data2) {
                        self.set_temperature_and_magneto_data_rate (if data1 != 0 { true } else { false }, data_rate);
                        ReturnCode::SUCCESS
                    }
                    else
                    {
                        ReturnCode::EINVAL
                    }
				}
				else
				{
					ReturnCode::EBUSY
				}
			}
			// Set Magnetometer Range
            5 => {
				if self.state.get () == State::Idle {
					if let Some (range) = Lsm303dlhcRange::from_usize(data1) {
                        self.set_range(range);
                        ReturnCode::SUCCESS
                    }
                    else
                    {
                        ReturnCode::EINVAL
                    }
				}
				else
				{
					ReturnCode::EBUSY
				}
            }
			// Read Acceleration XYZ
            6 => {
				if self.state.get () == State::Idle {
					self.read_acceleration_xyz ();
					ReturnCode::SUCCESS
				}
				else
				{
					ReturnCode::EBUSY
				}
            }
            // Read Temperature
            7 => {
				if self.state.get () == State::Idle {
					self.read_temperature ();
					ReturnCode::SUCCESS
				}
				else
				{
					ReturnCode::EBUSY
				}
			}
			8 => {
				if self.state.get () == State::Idle {
					self.read_magnetometer_xyz ();
					ReturnCode::SUCCESS
				}
				else
				{
					ReturnCode::EBUSY
				}
            }
            // default
            _ => ReturnCode::ENOSUPPORT,
        }
    }

    fn subscribe(
        &self,
        subscribe_num: usize,
        callback: Option<Callback>,
        _app_id: AppId,
    ) -> ReturnCode {
        match subscribe_num {
            0 /* set the one shot callback */ => {
				self.callback.insert (callback);
				ReturnCode::SUCCESS
			},
            // default
            _ => ReturnCode::ENOSUPPORT,
        }
    }
}
