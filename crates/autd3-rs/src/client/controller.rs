use std::future::poll_fn;
use std::sync::Arc;

use async_lock::Mutex as AsyncMutex;
use autd3_cpu_wire::payload::FirmwareInfo;
use zerocopy::FromBytes;

use crate::commands::operation::{Distribution, FixedCompletionTime};
use crate::commands::{Command, Pattern, SetSilencer};
use crate::datagram::{Datagram, Frame, Frames};
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::telemetry::Telemetry;
use crate::udp::{StateChecker, TransportOption};
use crate::value::{Intensity, SysTime};
use autd3_rs_core::BusStats;

use super::completion::{self, ResponseFuture};
use super::driver::Driver;
use super::exec_at::{self, ExecAt};
use super::session::Session;

pub struct Controller {
    geometry: Geometry,
    session: Arc<Session>,
    send_serializer: AsyncMutex<()>,
}

impl Controller {
    pub fn open(geometry: &Geometry, option: &TransportOption) -> Result<(Self, Driver), Error> {
        let session = Arc::new(Session::open(geometry, option)?);
        let driver = Driver::new(Arc::clone(&session));
        let controller = Self {
            geometry: geometry.clone(),
            session,
            send_serializer: AsyncMutex::new(()),
        };
        Ok((controller, driver))
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.geometry.num_devices()
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    #[must_use]
    pub fn bus_stats(&self) -> BusStats {
        self.session.stats()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        self.session.state_checker()
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.session.device_time().ok_or(Error::DeviceTimeUnknown)
    }

    pub fn send<C: Command>(&self, cmd: C) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        let frames = Frames::encode(&self.geometry, cmd);
        async move {
            for frame in &frames? {
                self.send_frame(frame).await?.await?.check()?;
            }
            Ok(())
        }
    }

    pub async fn send_frame(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        self.send_frame_at(frame, ExecAt::Immediate).await
    }

    pub async fn send_frame_at(
        &self,
        frame: Frame<'_>,
        exec: ExecAt,
    ) -> Result<ResponseFuture, Error> {
        let datagrams = frame.datagrams();
        let expected = frame.distribution().num_datagrams(self.num_devices());
        if datagrams.len() != expected {
            return Err(PayloadError::DatagramCountMismatch {
                expected,
                got: datagrams.len(),
            }
            .into());
        }
        if let Some((index, datagram)) = datagrams
            .iter()
            .enumerate()
            .find(|(_, datagram)| datagram.payload_len > PAYLOAD_BYTES)
        {
            return Err(PayloadError::DatagramPayloadTooLong {
                index,
                len: datagram.payload_len,
                max: PAYLOAD_BYTES,
            }
            .into());
        }
        let _serialized = self.send_serializer.lock().await;
        if let Some(e) = self.session.closed_error() {
            return Err(e);
        }
        let (completion, response) = completion::channel();
        let mut completion = Some(completion);
        let msg_id = poll_fn(|cx| self.session.poll_enqueue(cx, &mut completion)).await?;
        let exec_at = match exec_at::schedule(exec, self.session.lost_timeout(), || {
            self.session.device_time()
        }) {
            Ok(exec_at) => exec_at,
            Err(e) => {
                self.session.cancel(msg_id);
                return Err(e);
            }
        };
        tracing::trace!(msg_id, cmd = ?datagrams[0].cmd, dist = ?frame.distribution(), "sending frame");
        self.session.send_frame(msg_id, frame, exec_at)?;
        Ok(response)
    }

    pub fn send_heartbeat(&self) -> Result<(), Error> {
        if let Some(e) = self.session.closed_error() {
            return Err(e);
        }
        self.session.send_heartbeat()
    }

    async fn silent_stop(&self) -> Result<(), Error> {
        tracing::debug!("sending silent stop");
        let silenced = self
            .send(SetSilencer::new(FixedCompletionTime {
                strict_mode: false,
                ..Default::default()
            }))
            .await;
        if let Err(e) = &silenced {
            tracing::warn!(error = %e, "could not set the silencer before stopping");
        }
        let phases = self.geometry.phase_buffer();
        let stopped = self.send(Pattern::new(&phases, Intensity::MIN)).await;
        silenced.and(stopped)
    }

    async fn read_values<T>(
        &self,
        cmd: Cmd,
        parse: impl Fn(&[u8]) -> Option<T>,
    ) -> Result<Vec<T>, Error> {
        let datagram = [Datagram::no_payload(cmd)];
        let response = self
            .send_frame(Frame::new(Distribution::Broadcast, &datagram))
            .await?
            .await?;
        response.check()?;
        response.parse_each_device(parse)
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        self.read_values(Cmd::ReadFirmwareInfo, |value| {
            FirmwareInfo::read_from_prefix(value)
                .ok()
                .map(|(info, _)| FirmwareVersion::from_info(info))
        })
        .await
    }

    pub async fn read_fpga_state(&self) -> Result<Vec<FpgaState>, Error> {
        self.read_values(Cmd::ReadFpgaState, |value| {
            value.first().copied().map(FpgaState)
        })
        .await
    }

    pub async fn read_telemetry(&self) -> Result<Vec<Telemetry>, Error> {
        self.read_values(Cmd::ReadTelemetry, Telemetry::parse).await
    }

    pub async fn close(&self) -> Result<(), Error> {
        tracing::debug!("closing controller");
        if let Some(e) = self.session.closed_error() {
            return Err(e);
        }
        let stopped = self.silent_stop().await;
        self.shutdown();
        stopped
    }

    pub(crate) fn shutdown(&self) {
        self.session.request_stop();
        if !self.session.wake() {
            tracing::warn!("could not wake the driver; it stops at its next deadline");
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}
