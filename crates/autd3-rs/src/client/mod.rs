pub(crate) mod completion;
mod device_clock;
mod exec_at;
mod liveness;
pub(crate) mod open;
mod session;

pub use completion::ResponseFuture;
pub use exec_at::ExecAt;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::payload::FirmwareInfo;
use autd3_cpu_wire::update::RunningImage;
use tokio::task::JoinHandle;
use zerocopy::FromBytes;

use crate::commands::Command;
use crate::commands::operation::Distribution;
use crate::datagram::{Datagram, Frame, Frames};
use crate::device_status::DeviceStatus;
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::telemetry::Telemetry;
use crate::udp::TransportOption;
use crate::value::SysTime;

use session::Session;

const GRANDMASTER_UNIT_ID: u8 = 0;

fn unit_id_of(device: usize) -> u8 {
    u8::try_from(device).expect("at most 255 devices")
}

struct CloseOnDrop(Arc<Session>);

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.close(Error::Closed);
    }
}

fn run_io(session: Arc<Session>) -> impl Future<Output = Result<(), Error>> + Send {
    let closes_the_session = CloseOnDrop(session);
    async move {
        let session = &closes_the_session.0;
        session.run_io().await.map_err(|cause| {
            let error = Error::Network(cause);
            session.close(error.clone());
            error
        })
    }
}

pub struct Client {
    geometry: Geometry,
    session: Arc<Session>,
    send_serializer: tokio::sync::Mutex<()>,
    io_task: Mutex<Option<JoinHandle<Result<(), Error>>>>,
}

impl Client {
    pub async fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, Error> {
        let session = Session::open(geometry, option).await?;
        tracing::info!(num_devices = geometry.num_devices(), "client opened");
        Ok(Self::with_session(geometry.clone(), session))
    }

    fn with_session(geometry: Geometry, session: Session) -> Self {
        let session = Arc::new(session);
        let io_task = tokio::spawn(run_io(Arc::clone(&session)));
        Self {
            geometry,
            session,
            send_serializer: tokio::sync::Mutex::new(()),
            io_task: Mutex::new(Some(io_task)),
        }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.geometry.num_devices()
    }

    #[must_use]
    pub fn geometry(&self) -> &Geometry {
        &self.geometry
    }

    pub fn device_status(&self) -> Result<Vec<DeviceStatus>, Error> {
        let state = self.session.lock_state();
        if let Some(error) = self.session.closed_error_of(&state) {
            return Err(error);
        }
        Ok(state.device_status())
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.session
            .lock_state()
            .device_clock
            .now(Instant::now())
            .ok_or(Error::DeviceTimeUnknown)
    }

    #[must_use]
    pub fn ack_timeout(&self) -> Duration {
        self.session.lock_state().timeouts.ack
    }

    pub fn set_ack_timeout(&self, timeout: Duration) {
        self.session.lock_state().timeouts.ack = timeout;
        self.session.wake_io();
    }

    #[must_use]
    pub fn lost_timeout(&self) -> Duration {
        self.session.lock_state().timeouts.lost
    }

    pub fn set_lost_timeout(&self, timeout: Duration) {
        self.session.lock_state().timeouts.lost = timeout;
        self.session.wake_io();
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
        if let Some(e) = self.session.closed_error_of(&self.session.lock_state()) {
            return Err(e);
        }
        let (enqueued, response) = self.session.enqueue().await?;
        let (lost_timeout, device_time) = {
            let state = self.session.lock_state();
            (state.timeouts.lost, state.device_clock.now(Instant::now()))
        };
        let exec_at = exec_at::schedule(exec, lost_timeout, device_time)?;
        tracing::trace!(msg_id = enqueued.msg_id(), cmd = ?datagrams[0].cmd, dist = ?frame.distribution(), "sending frame");
        self.session.send_frame(enqueued, frame, exec_at).await?;
        Ok(response)
    }

    pub async fn send_heartbeat(&self) -> Result<(), Error> {
        self.session.send_heartbeat().await
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

    #[doc(hidden)]
    pub async fn read_running_image(&self) -> Result<Vec<Option<RunningImage>>, Error> {
        let datagram = [Datagram::no_payload(Cmd::ReadRunningImage)];
        let response = self
            .send_frame(Frame::new(Distribution::Broadcast, &datagram))
            .await?
            .await?;
        response.running_images()
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
        tracing::debug!("closing client");
        let closed = self
            .session
            .closed_error_of(&self.session.lock_state())
            .map_or(Ok(()), Err);
        self.session.request_stop();
        let io_task = self
            .io_task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(io_task) = io_task else {
            return Err(closed.err().unwrap_or(Error::Closed));
        };
        let stopped = io_task.await.unwrap_or_else(|e| {
            tracing::error!("the io task did not finish: {e}");
            Ok(())
        });
        closed.and(stopped)
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.session.request_stop();
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::Nop;
    use crate::test_utils::test_geometry;

    use super::session::tests::{TIMEOUTS, session_with_device};
    use super::*;

    async fn client() -> Client {
        let (session, _device) = session_with_device(TIMEOUTS).await;
        Client::with_session(test_geometry(1), session)
    }

    #[tokio::test]
    async fn closing_stops_the_io_task_and_a_second_close_reports_it() {
        let client = client().await;

        client.close().await.unwrap();

        assert!(client.session.lock_state().closed_error().is_some());
        assert!(client.io_task.lock().unwrap().is_none());
        assert!(matches!(client.close().await, Err(Error::Closed)));
        assert!(matches!(client.send(Nop).await, Err(Error::Closed)));
    }

    #[tokio::test]
    async fn dropping_the_client_stops_the_io_task() {
        let client = client().await;
        let session = Arc::clone(&client.session);
        let io_task = client.io_task.lock().unwrap().take().unwrap();

        drop(client);

        assert!(io_task.await.unwrap().is_ok());
        assert!(matches!(
            session.lock_state().closed_error(),
            Some(Error::Closed)
        ));
    }

    #[tokio::test]
    async fn cancelling_the_io_task_closes_the_session() {
        let client = client().await;
        let io_task = client.io_task.lock().unwrap().take().unwrap();

        io_task.abort();
        let _ = io_task.await;

        assert!(matches!(
            client.session.lock_state().closed_error(),
            Some(Error::Closed)
        ));
    }
}
