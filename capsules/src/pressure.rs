//! Provides userspace with access to pressure sensors.
//!
//! Userspace Interface
//! -------------------
//!
//! ### `subscribe` System Call
//!
//! The `subscribe` system call supports the single `subscribe_number` zero,
//! which is used to provide a callback that will return back the result of
//! a pressure sensor reading.
//! The `subscribe`call return codes indicate the following:
//!
//! * `SUCCESS`: the callback been successfully been configured.
//! * `ENOSUPPORT`: Invalid allow_num.
//! * `ENOMEM`: No sufficient memory available.
//! * `EINVAL`: Invalid address of the buffer or other error.
//!
//!
//! ### `command` System Call
//!
//! The `command` system call support one argument `cmd` which is used to specify the specific
//! operation, currently the following cmd's are supported:
//!
//! * `0`: check whether the driver exist
//! * `1`: read the pressure
//!
//!
//! The possible return from the 'command' system call indicates the following:
//!
//! * `SUCCESS`:    The operation has been successful.
//! * `EBUSY`:      The driver is busy.
//! * `ENOSUPPORT`: Invalid `cmd`.
//! * `ENOMEM`:     No sufficient memory available.
//! * `EINVAL`:     Invalid address of the buffer or other error.
//!
//! Usage
//! -----
//!
//! You need a device that provides the `hil::sensors::PressureDriver` trait.
//!
//! ```rust
//! # use kernel::static_init;
//!
//! let grant_cap = create_capability!(capabilities::MemoryAllocationCapability);
//! let grant_temperature = board_kernel.create_grant(&grant_cap);
//!
//! let temp = static_init!(
//!        capsules::pressure::PressureSensor<'static>,
//!        capsules::pressure::PressureSensor::new(si7021,
//!                                                 board_kernel.create_grant(&grant_cap)));
//!
//! kernel::hil::sensors::PressureDriver::set_client(si7021, temp);
//! ```

use core::cell::Cell;
use kernel::hil;
use kernel::ReturnCode;
use kernel::{AppId, Callback, Driver, Grant};

/// Syscall driver number.
use crate::driver;
pub const DRIVER_NUM: usize = driver::NUM::Pressure as usize;

#[derive(Default)]
pub struct App {
    callback: Option<Callback>,
    subscribed: bool,
}

pub struct PressureSensor<'a> {
    driver: &'a dyn hil::sensors::PressureDriver<'a>,
    apps: Grant<App>,
    busy: Cell<bool>,
}

impl<'a> PressureSensor<'a> {
    pub fn new(
        driver: &'a dyn hil::sensors::PressureDriver<'a>,
        grant: Grant<App>,
    ) -> PressureSensor<'a> {
        PressureSensor {
            driver: driver,
            apps: grant,
            busy: Cell::new(false),
        }
    }

    fn enqueue_command(&self, appid: AppId) -> ReturnCode {
        self.apps
            .enter(appid, |app, _| {
                if !self.busy.get() {
                    app.subscribed = true;
                    self.busy.set(true);
                    self.driver.read_pressure()
                } else {
                    ReturnCode::EBUSY
                }
            })
            .unwrap_or_else(|err| err.into())
    }

    fn configure_callback(&self, callback: Option<Callback>, app_id: AppId) -> ReturnCode {
        self.apps
            .enter(app_id, |app, _| {
                app.callback = callback;
                ReturnCode::SUCCESS
            })
            .unwrap_or_else(|err| err.into())
    }
}

impl hil::sensors::PressureClient for PressureSensor<'_> {
    fn callback(&self, temp_val: usize) {
        for cntr in self.apps.iter() {
            cntr.enter(|app, _| {
                if app.subscribed {
                    self.busy.set(false);
                    app.subscribed = false;
                    app.callback.map(|mut cb| cb.schedule(temp_val, 0, 0));
                }
            });
        }
    }
}

impl Driver for PressureSensor<'_> {
    fn subscribe(
        &self,
        subscribe_num: usize,
        callback: Option<Callback>,
        app_id: AppId,
    ) -> ReturnCode {
        match subscribe_num {
            // subscribe to pressure reading with callback
            0 => self.configure_callback(callback, app_id),
            _ => ReturnCode::ENOSUPPORT,
        }
    }

    fn command(&self, command_num: usize, _: usize, _: usize, appid: AppId) -> ReturnCode {
        match command_num {
            // check whether the driver exists!!
            0 => ReturnCode::SUCCESS,

            // read pressure
            1 => self.enqueue_command(appid),
            _ => ReturnCode::ENOSUPPORT,
        }
    }
}
