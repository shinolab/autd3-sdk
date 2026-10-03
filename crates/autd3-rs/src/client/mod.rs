pub(crate) mod completion;
mod config;
pub(crate) mod pool;

#[cfg(test)]
mod resync_tests;
#[cfg(test)]
mod tests;

pub use completion::ResponseFuture;
pub use config::{ClientConfig, MAX_DEVICES};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};

use autd3_rs_core::rt::oneshot;

use autd3_cpu_wire::payload::FirmwareInfo;
use zerocopy::FromBytes;

use crate::commands::Pattern;
use crate::commands::operation::{Clear, Distribution, Synchronize};
use crate::datagram::{Datagram, DatagramBuilder, Frame, Mirror, MirrorHandle};
use crate::driver::{CmdMessage, Connect, Connector, Link};
use crate::error::{Error, NetworkCause, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::mirror::FirmwareState;
use crate::protocol::Cmd;
use crate::response::Response;
use crate::telemetry::TelemetryCounters;
use crate::value::{Intensity, SysTime};
use autd3_rs_core::{BusStats, DeviceClock};

use completion::{CompletionPool, Reply};
use pool::SlotPool;

type DoneReceiver = oneshot::Receiver<Option<NetworkCause>>;

pub struct Client {
    link: Link,
    geometry: Arc<Geometry>,
    num_devices: usize,
    pool: Arc<SlotPool>,
    completions: Arc<CompletionPool>,
    done: std::sync::Mutex<Option<DoneReceiver>>,
    stopping: AtomicBool,
    mirror: MirrorHandle,
    device_clock: Option<DeviceClock>,
    stats: BusStats,
}

async fn closed_cause(done: DoneReceiver) -> Error {
    match done.await {
        Ok(Some(cause)) => Error::Network(cause),
        _ => Error::DriverClosed,
    }
}

impl Client {
    pub fn open(
        geometry: &Geometry,
        connector: Connector,
        config: ClientConfig,
    ) -> impl Future<Output = Result<Self, Error>> + Send + '_ {
        Box::pin(Self::open_impl(geometry, connector, config))
    }

    async fn open_impl(
        geometry: &Geometry,
        connector: Connector,
        config: ClientConfig,
    ) -> Result<Self, Error> {
        let Connector {
            link,
            done,
            num_devices,
            stats,
            device_clock,
        } = connector;
        let config = config.validate()?;
        if geometry.num_devices() != num_devices {
            return Err(PayloadError::GeometryDeviceMismatch {
                geometry: geometry.num_devices(),
                attached: num_devices,
            }
            .into());
        }

        let (hs_tx, hs_rx) = oneshot::channel::<Result<(), NetworkCause>>();
        if link
            .queue
            .push_connect(Connect {
                config: config.transport(),
                done: hs_tx,
            })
            .is_err()
        {
            return Err(closed_cause(done).await);
        }

        match hs_rx.await {
            Ok(Ok(())) => {
                tracing::debug!("driver handshake complete");
                let client = Self {
                    link,
                    geometry: Arc::new(geometry.clone()),
                    num_devices,
                    pool: SlotPool::new(num_devices, config.max_inflight.get()),
                    completions: CompletionPool::new(config.max_inflight.get()),
                    done: std::sync::Mutex::new(Some(done)),
                    stopping: AtomicBool::new(false),
                    mirror: MirrorHandle {
                        state: Arc::new(std::sync::Mutex::new(Mirror::Desynced)),
                        enabled: config.validate_state,
                    },
                    device_clock,
                    stats,
                };
                if let Err(e) = client
                    .check_firmware_version(config.require_supported_firmware)
                    .await
                {
                    let _ = client.close_impl(false).await;
                    return Err(e);
                }
                if let Err(e) = client.clear().await {
                    let _ = client.close_impl(false).await;
                    return Err(e);
                }
                if let Err(e) = client.synchronize().await {
                    let _ = client.close_impl(false).await;
                    return Err(e);
                }
                tracing::info!(num_devices, "client opened");
                Ok(client)
            }
            Ok(Err(cause)) => Err(Error::Network(cause)),
            Err(oneshot::Canceled) => Err(closed_cause(done).await),
        }
    }

    #[cfg(test)]
    pub(crate) async fn open_bus<B: crate::transport::Bus>(
        geometry: &Geometry,
        bus: B,
        config: ClientConfig,
    ) -> Result<Self, Error> {
        crate::driver::check_device_count(bus.num_devices())?;
        let (connector, _join) = spawn_driver(bus);
        Self::open(geometry, connector, config).await
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    #[must_use]
    pub fn bus_stats(&self) -> BusStats {
        self.stats.clone()
    }

    #[must_use]
    pub fn clock_offset_ns(&self) -> i64 {
        self.device_clock
            .as_ref()
            .and_then(DeviceClock::offset_ns)
            .unwrap_or(0)
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        Ok(SysTime::now()?.with_clock_offset(self.clock_offset_ns()))
    }

    #[must_use]
    pub fn datagram_builder<'a>(&self) -> DatagramBuilder<'a> {
        DatagramBuilder::with_mirror(
            Arc::clone(&self.geometry),
            self.mirror.clone(),
            self.device_clock.clone().into(),
        )
    }

    fn mark_desynced(&self) {
        self.mirror.desync();
    }

    fn mirror_for_response(&self) -> Option<MirrorHandle> {
        self.mirror.enabled.then(|| self.mirror.clone())
    }

    async fn clear(&self) -> Result<(), Error> {
        let datagrams = self.datagram_builder().push(Clear).build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        self.mirror.set(Mirror::Synced(vec![
            FirmwareState::boot_default();
            self.num_devices
        ]));
        Ok(())
    }

    async fn send_datagrams(&self, datagrams: &[Datagram]) -> Result<ResponseFuture, Error> {
        if datagrams.len() != self.num_devices {
            self.mark_desynced();
            return Err(PayloadError::DatagramCountMismatch {
                expected: self.num_devices,
                got: datagrams.len(),
            }
            .into());
        }
        tracing::trace!(cmd = ?datagrams[0].cmd, "sending per-device frame");
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::PerDevice);
        for (device, datagram) in datagrams.iter().enumerate() {
            slot.set(device, datagram);
        }
        self.dispatch(slot, Reply::Ack)
    }

    async fn send_broadcast(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        tracing::trace!(cmd = ?datagram.cmd, "sending broadcast frame");
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.set(0, datagram);
        self.dispatch(slot, Reply::Ack)
    }

    async fn send_broadcast_exclusive(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        tracing::trace!(cmd = ?datagram.cmd, "sending exclusive broadcast frame");
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.set(0, datagram);
        self.dispatch(slot, Reply::Value)
    }

    pub async fn send(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        match frame.distribution() {
            Distribution::Broadcast => self.send_broadcast(&frame.datagrams()[0]).await,
            Distribution::PerDevice => self.send_datagrams(frame.datagrams()).await,
        }
    }

    pub async fn send_checked(&self, frame: Frame<'_>) -> Result<(), Error> {
        self.send(frame).await?.await?.check()
    }

    fn dispatch(&self, slot: pool::Slot, reply: Reply) -> Result<ResponseFuture, Error> {
        let (response_tx, response_rx) =
            self.completions.channel(self.mirror_for_response(), reply);
        if self
            .link
            .queue
            .push_cmd(CmdMessage {
                frame: slot,
                response_tx,
                exclusive: reply.exclusive(),
            })
            .is_err()
        {
            tracing::warn!("the driver is closed; frame dropped");
            self.mark_desynced();
            return Err(Error::DriverClosed);
        }
        Ok(response_rx)
    }

    async fn synchronize(&self) -> Result<(), Error> {
        let datagrams = self.datagram_builder().push(Synchronize).build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), Error> {
        tracing::debug!("sending stop");
        let phases = self.geometry.phase_buffer();
        let intensities: Vec<Vec<Intensity>> = self
            .geometry
            .iter()
            .map(|d| vec![Intensity::MIN; d.num_transducers()])
            .collect();
        let datagrams = self
            .datagram_builder()
            .push(Pattern::new(&phases, &intensities))
            .build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        Ok(())
    }

    async fn read_broadcast(&self, cmd: Cmd) -> Result<Response, Error> {
        let response = self
            .send_broadcast_exclusive(&Datagram::no_payload(cmd))
            .await?
            .await?;
        response.check()?;
        Ok(response)
    }

    async fn read_values<T>(
        &self,
        cmd: Cmd,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<Vec<T>, Error> {
        let response = self.read_broadcast(cmd).await?;
        (0..self.num_devices)
            .map(|device| parse(response.value(device)).ok_or(Error::UnexpectedReply { device }))
            .collect()
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let versions = self
            .read_values(Cmd::ReadFirmwareInfo, |value| {
                FirmwareInfo::read_from_prefix(value)
                    .ok()
                    .map(|(info, _)| FirmwareVersion::from_info(info))
            })
            .await?;

        let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
        versions
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_supported())
            .for_each(|(device, version)| {
                tracing::warn!(
                    device,
                    "firmware {version} is outside the series supported by this SDK ({major}.{minor}.x); correct operation is not guaranteed"
                );
            });

        Ok(versions)
    }

    async fn check_firmware_version(&self, require_supported: bool) -> Result<(), Error> {
        let versions = match self.read_firmware_version().await {
            Ok(versions) => versions,
            Err(e) if require_supported => return Err(e),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "could not read the firmware version, so the series check was skipped"
                );
                return Ok(());
            }
        };
        if !require_supported {
            return Ok(());
        }
        versions
            .into_iter()
            .enumerate()
            .find(|(_, version)| !version.is_supported())
            .map_or(Ok(()), |(device, version)| {
                Err(Error::UnsupportedFirmware { device, version })
            })
    }

    pub async fn read_error_detail(&self) -> Result<Vec<u8>, Error> {
        self.read_values(Cmd::ReadErrorDetail, |value| value.first().copied())
            .await
    }

    pub async fn read_fpga_state(&self) -> Result<Vec<FpgaState>, Error> {
        self.read_values(Cmd::ReadFpgaState, |value| {
            value.first().copied().map(FpgaState)
        })
        .await
    }

    pub async fn read_telemetry(&self) -> Result<Vec<TelemetryCounters>, Error> {
        self.read_values(Cmd::ReadTelemetry, TelemetryCounters::parse)
            .await
    }

    pub async fn close(&self) -> Result<(), Error> {
        self.close_impl(true).await
    }

    async fn close_impl(&self, stop: bool) -> Result<(), Error> {
        tracing::debug!("closing client");
        let stopped = if stop && !self.stopping.swap(true, Ordering::AcqRel) {
            self.stop().await
        } else {
            Ok(())
        };
        self.link.queue.request_close();
        let done = self
            .done
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let closed = match done {
            Some(done) => match done.await {
                Ok(None) => Ok(()),
                Ok(Some(cause)) => Err(Error::Network(cause)),
                Err(oneshot::Canceled) => Err(Error::DriverClosed),
            },
            None => Ok(()),
        };
        stopped.and(closed)
    }
}

#[cfg(test)]
pub(crate) fn spawn_driver<B: crate::transport::Bus>(
    bus: B,
) -> (Connector, std::thread::JoinHandle<Result<(), Error>>) {
    let (mut engine, connector) = crate::driver::attach(bus);
    let join = std::thread::spawn(move || engine.run());
    (connector, join)
}
