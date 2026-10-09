pub(crate) mod completion;
mod config;
mod controller;
mod driver;
pub(crate) mod pool;

pub use completion::{ResponseFuture, StreamFuture};
pub use config::{ClientConfig, MAX_DEVICES};
pub use controller::Controller;
pub use driver::Driver;

use std::sync::{Mutex, PoisonError};
use std::thread::JoinHandle;

use autd3_rs_core::BusStats;

use crate::commands::Command;
use crate::datagram::Frame;
use crate::error::{Error, NetworkCause};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::telemetry::TelemetryCounters;
use crate::udp::{StateChecker, TransportOption};
use crate::value::SysTime;

pub struct Client {
    controller: Controller,
    driver: Mutex<Option<JoinHandle<Result<(), Error>>>>,
}

impl Client {
    pub fn open<'a>(
        geometry: &'a Geometry,
        option: &'a TransportOption,
        config: ClientConfig,
    ) -> impl Future<Output = Result<Self, Error>> + Send + 'a {
        Box::pin(Self::open_impl(geometry, option, config))
    }

    async fn open_impl(
        geometry: &Geometry,
        option: &TransportOption,
        config: ClientConfig,
    ) -> Result<Self, Error> {
        let (controller, driver) = Controller::open(geometry, option, config)?;
        let thread = std::thread::Builder::new()
            .name("autd3-io".into())
            .spawn(move || driver.run())
            .map_err(|e| Error::Network(NetworkCause::new(e)))?;
        let client = Self {
            controller,
            driver: Mutex::new(Some(thread)),
        };
        client.controller.initialize().await?;
        tracing::info!(num_devices = client.num_devices(), "client opened");
        Ok(client)
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.controller.num_devices()
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        self.controller.geometry()
    }

    #[must_use]
    pub fn bus_stats(&self) -> BusStats {
        self.controller.bus_stats()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        self.controller.state_checker()
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.controller.device_time_now()
    }

    pub fn send<'a, C: Command<'a>>(
        &self,
        cmd: C,
    ) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        self.controller.send(cmd)
    }

    pub fn send_streaming<'a, C: Command<'a>>(
        &self,
        cmd: C,
    ) -> impl Future<Output = Result<StreamFuture, Error>> + use<'_, C> {
        self.controller.send_streaming(cmd)
    }

    pub async fn send_frame(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        self.controller.send_frame(frame).await
    }

    pub async fn silent_stop(&self) -> Result<(), Error> {
        self.controller.silent_stop().await
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        self.controller.read_firmware_version().await
    }

    pub async fn read_fpga_state(&self) -> Result<Vec<FpgaState>, Error> {
        self.controller.read_fpga_state().await
    }

    pub async fn read_telemetry(&self) -> Result<Vec<TelemetryCounters>, Error> {
        self.controller.read_telemetry().await
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.controller.close().await?;
        self.join()
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

impl Drop for Client {
    fn drop(&mut self) {
        self.controller.shutdown();
        let _ = self.join();
    }
}
