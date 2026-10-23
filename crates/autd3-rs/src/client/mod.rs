pub(crate) mod completion;
mod controller;
mod device_clock;
mod driver;
mod exec_at;
mod liveness;
pub(crate) mod open;
mod session;

pub use completion::ResponseFuture;
pub use controller::Controller;
pub use driver::Driver;
pub use exec_at::ExecAt;

use std::ops::Deref;
use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use crate::error::Error;
use crate::geometry::Geometry;
use crate::udp::{TransportOption, UdpError};

const GRANDMASTER_UNIT_ID: u8 = 0;

pub struct Client {
    controller: Controller,
    driver: Mutex<Option<JoinHandle<Result<(), Error>>>>,
}

impl Client {
    pub fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, Error> {
        let (controller, driver) = Controller::open(geometry, option)?;
        let thread = std::thread::Builder::new()
            .name("autd3-io".into())
            .spawn(move || driver.run())
            .map_err(|e| Error::from(UdpError::from(e)))?;
        let client = Self {
            controller,
            driver: Mutex::new(Some(thread)),
        };
        tracing::info!(num_devices = client.num_devices(), "client opened");
        Ok(client)
    }

    pub fn close(&self) -> Result<(), Error> {
        let closed = self.controller.close();
        let joined = self.join();
        closed.and(joined)
    }

    fn join(&self) -> Result<(), Error> {
        let thread = self
            .driver
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match thread.map(JoinHandle::join) {
            Some(Ok(outcome)) => outcome,
            Some(Err(_)) => {
                tracing::error!("the io thread panicked");
                Ok(())
            }
            None => Ok(()),
        }
    }
}

impl Deref for Client {
    type Target = Controller;

    fn deref(&self) -> &Controller {
        &self.controller
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.controller.shutdown();
        let _ = self.join();
    }
}
