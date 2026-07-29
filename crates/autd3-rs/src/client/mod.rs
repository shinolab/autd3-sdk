mod completion;
mod config;
mod pool;
mod rt;

#[cfg(test)]
mod tests;

pub use completion::ResponseFuture;
pub use config::{ClientConfig, MAX_DEVICES};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;

use tokio::sync::{mpsc, oneshot};

use autd3_cpu_wire::payload::ReadTelemetryPayload;
use zerocopy::FromBytes;

use crate::commands::Pattern;
use crate::commands::operation::{Clear, Distribution, Synchronize};
use crate::datagram::{Datagram, DatagramBuilder, Frame, Mirror, MirrorHandle};
use crate::error::{Error, PayloadError};
use crate::firmware_version::{FirmwareVersion, Version};
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::link::{DcClock, IntoLink, Link};
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, DeviceErrorCode};
use crate::telemetry::Telemetry;
use crate::value::DcSysTime;

use completion::CompletionPool;
use pool::SlotPool;
use rt::CmdMessage;

pub struct Client {
    cmd_tx: mpsc::Sender<CmdMessage>,
    geometry: Arc<Geometry>,
    num_devices: usize,
    pool: Arc<SlotPool>,
    completions: Arc<CompletionPool>,
    join: std::sync::Mutex<Option<JoinHandle<()>>>,
    closed: Arc<AtomicBool>,
    mirror: MirrorHandle,
    dc_clock: Option<DcClock>,
}

impl Client {
    pub fn open<'g, T: IntoLink + 'g>(
        geometry: &'g Geometry,
        link: T,
        config: ClientConfig,
    ) -> impl Future<Output = Result<Self, Error>> + Send + 'g {
        Box::pin(async move {
            Self::open_impl(geometry, link, config)
                .await
                .map(|(client, _checker)| client)
        })
    }

    pub fn open_with_checker<'g, T: IntoLink + 'g>(
        geometry: &'g Geometry,
        link: T,
        config: ClientConfig,
    ) -> impl Future<Output = Result<(Self, <T::Link as Link>::Checker), Error>> + Send + 'g {
        Box::pin(Self::open_impl(geometry, link, config))
    }

    async fn open_impl<T: IntoLink>(
        geometry: &Geometry,
        link: T,
        config: ClientConfig,
    ) -> Result<(Self, <T::Link as Link>::Checker), Error> {
        let config = config.validate()?;
        let link = link.into_link(geometry).await?;
        let num_devices = link.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        if geometry.num_devices() != num_devices {
            return Err(PayloadError::GeometryDeviceMismatch {
                geometry: geometry.num_devices(),
                link: num_devices,
            }
            .into());
        }

        let checker = link.state_checker();
        let dc_clock = link.dc_clock();
        let pool = SlotPool::new(num_devices, config.max_inflight.get());
        let completions = CompletionPool::new(config.max_inflight.get());

        let (cmd_tx, cmd_rx) = mpsc::channel::<CmdMessage>(1);
        let (hs_done_tx, hs_done_rx) = oneshot::channel::<Result<(), String>>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_rt = Arc::clone(&closed);

        let join = std::thread::Builder::new()
            .name("autd3-rs-rt".to_owned())
            .spawn(move || {
                rt::run_rt_thread(link, cmd_rx, config, hs_done_tx, closed_for_rt);
            })
            .map_err(|e| Error::Link(format!("failed to spawn RT thread: {e}")))?;

        match hs_done_rx.await {
            Ok(Ok(())) => {
                tracing::debug!("RT thread handshake complete");
                let client = Self {
                    cmd_tx,
                    geometry: Arc::new(geometry.clone()),
                    num_devices,
                    pool,
                    completions,
                    join: std::sync::Mutex::new(Some(join)),
                    closed,
                    mirror: MirrorHandle {
                        state: Arc::new(std::sync::Mutex::new(Mirror::Desynced)),
                        enabled: config.validate_state,
                    },
                    dc_clock,
                };
                if let Err(e) = client.clear().await {
                    let _ = client.close().await;
                    return Err(e);
                }
                if let Err(e) = client.synchronize().await {
                    let _ = client.close().await;
                    return Err(e);
                }
                tracing::info!(num_devices, "client opened");
                Ok((client, checker))
            }
            Ok(Err(msg)) => {
                let _ = wait_thread(join).await;
                Err(Error::Link(msg))
            }
            Err(_) => {
                let _ = wait_thread(join).await;
                Err(Error::RtClosed)
            }
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.num_devices
    }

    #[must_use]
    pub fn dc_offset_ns(&self) -> i64 {
        self.dc_clock
            .as_ref()
            .and_then(DcClock::offset_ns)
            .unwrap_or(0)
    }

    #[must_use]
    pub fn dc_sys_time(&self) -> DcSysTime {
        DcSysTime::now().with_dc_offset(self.dc_offset_ns())
    }

    #[must_use]
    pub fn datagram_builder<'a>(&self) -> DatagramBuilder<'a> {
        DatagramBuilder::with_mirror(
            Arc::clone(&self.geometry),
            self.mirror.clone(),
            self.dc_offset_ns(),
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
            slot.payload_mut(device).copy_from_slice(&datagram.payload);
            slot.set_cmd(device, datagram.cmd);
        }
        self.dispatch(slot, false).await
    }

    async fn send_broadcast(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        tracing::trace!(cmd = ?datagram.cmd, "sending broadcast frame");
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.payload_mut(0).copy_from_slice(&datagram.payload);
        slot.set_cmd(0, datagram.cmd);
        self.dispatch(slot, false).await
    }

    async fn send_broadcast_exclusive(&self, datagram: &Datagram) -> Result<ResponseFuture, Error> {
        tracing::trace!(cmd = ?datagram.cmd, "sending exclusive broadcast frame");
        let mut slot = self.pool.acquire().await;
        slot.reset(Distribution::Broadcast);
        slot.payload_mut(0).copy_from_slice(&datagram.payload);
        slot.set_cmd(0, datagram.cmd);
        self.dispatch(slot, true).await
    }

    pub async fn send(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        match frame.distribution() {
            Distribution::Broadcast => self.send_broadcast(&frame.datagrams()[0]).await,
            Distribution::PerDevice => self.send_datagrams(frame.datagrams()).await,
        }
    }

    pub async fn send_checked(&self, frame: Frame<'_>) -> Result<(), Error> {
        let result = self.send(frame).await?.await?.check();
        if let Err(e) = &result {
            tracing::warn!(error = %e, "device reported an error; mirror desynced");
            self.mark_desynced();
        }
        result
    }

    async fn dispatch(&self, slot: pool::Slot, exclusive: bool) -> Result<ResponseFuture, Error> {
        let (response_tx, response_rx) = self.completions.channel(self.mirror_for_response());
        if self
            .cmd_tx
            .send(CmdMessage {
                frame: slot,
                response_tx,
                exclusive,
            })
            .await
            .is_err()
        {
            tracing::warn!("RT thread is closed; frame dropped");
            self.mark_desynced();
            return Err(Error::RtClosed);
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
        let buf = self.geometry.pattern_buffer();
        let datagrams = self.datagram_builder().push(Pattern::new(&buf)).build()?;
        for frame in &datagrams {
            self.send_checked(frame).await?;
        }
        Ok(())
    }

    async fn read_broadcast(&self, cmd: Cmd) -> Result<Vec<u8>, Error> {
        self.read_broadcast_with(&Datagram::no_payload(cmd)).await
    }

    async fn read_broadcast_with(&self, datagram: &Datagram) -> Result<Vec<u8>, Error> {
        Ok(self
            .send_broadcast_exclusive(datagram)
            .await?
            .await?
            .data()
            .to_vec())
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        const UNKNOWN_CMD: u8 = DeviceErrorCode::UnknownCmd as u8;

        let cpu_major = self.read_broadcast(Cmd::ReadCpuFwVersionMajor).await?;
        let cpu_minor = self.read_broadcast(Cmd::ReadCpuFwVersionMinor).await?;
        let cpu_patch = self.read_broadcast(Cmd::ReadCpuFwVersionPatch).await?;

        let err_before = self.read_broadcast(Cmd::ReadErrorDetail).await?;
        let fpga_major = self.read_broadcast(Cmd::ReadFpgaFwVersionMajor).await?;
        let fpga_minor = self.read_broadcast(Cmd::ReadFpgaFwVersionMinor).await?;
        let fpga_patch = self.read_broadcast(Cmd::ReadFpgaFwVersionPatch).await?;
        let fpga_functions = self.read_broadcast(Cmd::ReadFpgaFunctions).await?;
        let err_after = self.read_broadcast(Cmd::ReadErrorDetail).await?;

        Ok((0..cpu_major.len())
            .map(|i| {
                let (fpga, function_bits) = if err_after[i] == UNKNOWN_CMD {
                    if err_before[i] == UNKNOWN_CMD {
                        tracing::warn!(
                            device = i,
                            "FPGA firmware version is unknown: {} was already latched before the query, so it cannot be attributed to it",
                            DeviceErrorCode::UnknownCmd.describe()
                        );
                    } else {
                        tracing::warn!(
                            device = i,
                            "FPGA firmware version is unknown: {}",
                            DeviceErrorCode::UnknownCmd.describe()
                        );
                    }
                    (Version::UNKNOWN, 0)
                } else {
                    (
                        Version {
                            major: fpga_major[i],
                            minor: fpga_minor[i],
                            patch: fpga_patch[i],
                        },
                        fpga_functions[i],
                    )
                };
                FirmwareVersion {
                    cpu: Version {
                        major: cpu_major[i],
                        minor: cpu_minor[i],
                        patch: cpu_patch[i],
                    },
                    fpga,
                    function_bits,
                }
            })
            .collect())
    }

    pub async fn read_error_detail(&self) -> Result<Vec<u8>, Error> {
        self.read_broadcast(Cmd::ReadErrorDetail).await
    }

    pub async fn read_fpga_state(&self) -> Result<Vec<FpgaState>, Error> {
        Ok(self
            .read_broadcast(Cmd::ReadFpgaState)
            .await?
            .into_iter()
            .map(FpgaState)
            .collect())
    }

    pub async fn read_telemetry(&self, counter: Telemetry) -> Result<Vec<u8>, Error> {
        let mut datagram = Datagram::no_payload(Cmd::ReadTelemetry);
        let (p, _) = ReadTelemetryPayload::mut_from_prefix(&mut datagram.payload).unwrap();
        p.counter_id = counter.as_u8();
        self.read_broadcast_with(&datagram).await
    }

    pub async fn close(&self) -> Result<(), Error> {
        tracing::debug!("closing client");
        self.closed.store(true, Ordering::Release);
        let join = self
            .join
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            wait_thread(join).await
        } else {
            Ok(())
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let join = self
            .join
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            let _ = join.join();
        }
    }
}

async fn wait_thread(join: JoinHandle<()>) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || join.join())
        .await
        .map_err(|e| Error::Link(format!("RT thread join failed: {e}")))?
        .map_err(|_| Error::Link("RT thread panicked".to_owned()))
}
