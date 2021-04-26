//! Driver for STM ADC MCU temperature sensor

use core::cell::Cell;
use kernel::common::cells::OptionalCell;
use kernel::hil::adc;
use kernel::hil::sensors;
use kernel::ErrorCode;

use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::Temperature as usize;

#[derive(Copy, Clone, PartialEq)]
pub enum Status {
    Read,
    Idle,
}

pub struct TemperatureRp4020<'a> {
    adc: &'a dyn adc::AdcChannel,
    slope: f32,
    v_27: f32,
    temperature_client: OptionalCell<&'a dyn sensors::TemperatureClient>,
    status: Cell<Status>,
}

impl<'a> TemperatureRp4020<'a> {
    /// slope - device specific slope found in datasheet
    /// v_27 - voltage at 27 degrees Celsius found in datasheet
    pub fn new(adc: &'a dyn adc::AdcChannel, slope: f32, v_27: f32) -> TemperatureRp4020<'a> {
        TemperatureRp4020 {
            adc: adc,
            slope: slope,
            v_27: v_27,
            temperature_client: OptionalCell::empty(),
            status: Cell::new(Status::Idle),
        }
    }
}

impl<'a> adc::Client for TemperatureRp4020<'a> {
    fn sample_ready(&self, sample: u16) {
        self.status.set(Status::Idle);
        self.temperature_client.map(|client| {
            client.callback(
                ((27.0 - (((sample as f32 * 3.3 / 4095.0) - self.v_27) * 1000.0 / self.slope))
                    * 100.0) as usize,
            );
        });
    }
}

impl<'a> sensors::TemperatureDriver<'a> for TemperatureRp4020<'a> {
    fn set_client(&self, temperature_client: &'a dyn sensors::TemperatureClient) {
        self.temperature_client.replace(temperature_client);
    }

    fn read_temperature(&self) -> Result<(), ErrorCode> {
        if self.status.get() == Status::Idle {
            self.status.set(Status::Read);
            let _ = self.adc.sample();
            Ok(())
        } else {
            Err(ErrorCode::BUSY)
        }
    }
}