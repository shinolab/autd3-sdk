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
use tokio::task::{AbortHandle, JoinHandle};
use zerocopy::FromBytes;

use crate::commands::Command;
use crate::commands::operation::Distribution;
use crate::datagram::{Datagram, Frame, Frames};
use crate::device_status::DeviceStatus;
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::protocol::{Cmd, DeviceErrorCode, PAYLOAD_BYTES};
use crate::telemetry::Telemetry;
use crate::udp::TransportOption;
use crate::value::SysTime;

use session::Session;

const GRANDMASTER_UNIT_ID: usize = 0;

pub struct Client {
    geometry: Geometry,
    session: Arc<Session>,
    send_serializer: tokio::sync::Mutex<()>,
    io_task: Mutex<Option<JoinHandle<Result<(), Error>>>>,
    heartbeat_task: Option<AbortHandle>,
}

impl Client {
    pub async fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, Error> {
        let session = Arc::new(Session::open(geometry, option).await?);
        tracing::info!(num_devices = geometry.num_devices(), "client opened");
        let io_task = tokio::spawn({
            struct CloseOnDrop(Arc<Session>);
            impl Drop for CloseOnDrop {
                fn drop(&mut self) {
                    self.0.close(Error::Closed);
                }
            }
            let closes_the_session = CloseOnDrop(Arc::clone(&session));
            async move {
                let session = &closes_the_session.0;
                session.run_io().await.map_err(|cause| {
                    let error = Error::Network(cause);
                    session.close(error.clone());
                    error
                })
            }
        });
        let heartbeat_task = option.heartbeat_interval.map(|interval| {
            let session = Arc::clone(&session);
            tokio::spawn(async move {
                let mut interval = (interval != Duration::ZERO).then(|| {
                    let mut interval = tokio::time::interval(interval);
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    interval
                });
                while session.send_heartbeat().await.is_ok() {
                    if let Some(interval) = &mut interval {
                        interval.tick().await;
                    } else {
                        tokio::task::yield_now().await;
                    }
                }
            })
            .abort_handle()
        });
        Ok(Self {
            geometry: geometry.clone(),
            session,
            send_serializer: tokio::sync::Mutex::new(()),
            io_task: Mutex::new(Some(io_task)),
            heartbeat_task,
        })
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
        self.session.lock_state().ack_timeout
    }

    pub fn set_ack_timeout(&self, timeout: Duration) {
        self.session.lock_state().ack_timeout = timeout;
        self.session.wake_io();
    }

    #[must_use]
    pub fn lost_timeout(&self) -> Duration {
        self.session.lock_state().lost_timeout
    }

    pub fn set_lost_timeout(&self, timeout: Duration) {
        self.session.lock_state().lost_timeout = timeout;
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
            (state.lost_timeout, state.device_clock.now(Instant::now()))
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
        response
            .error_codes()
            .iter()
            .enumerate()
            .map(|(device, &code)| match DeviceErrorCode::from_u8(code) {
                Some(DeviceErrorCode::UnknownCmd) => Ok(None),
                Some(DeviceErrorCode::None) => response
                    .reply_data_of(device)
                    .first()
                    .map(|&raw| Some(RunningImage::from_u8(raw).unwrap_or(RunningImage::Unknown)))
                    .ok_or(Error::UnexpectedReply { device }),
                _ => Err(Error::DeviceError { device, code }),
            })
            .collect()
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

    fn request_stop(&self) {
        self.session.request_stop();
        if let Some(heartbeat_task) = &self.heartbeat_task {
            heartbeat_task.abort();
        }
    }

    pub async fn close(&self) -> Result<(), Error> {
        tracing::debug!("closing client");
        let closed = self
            .session
            .closed_error_of(&self.session.lock_state())
            .map_or(Ok(()), Err);
        self.request_stop();
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
        self.request_stop();
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};

    use autd3_cpu_wire::udp::{Flags, Header, Kind, Status, UnitInfo};
    use autd3_rs_core::Interface;
    use zerocopy::IntoBytes;
    use zerocopy::little_endian::I32;

    use crate::commands::Nop;
    use crate::test_utils::test_geometry;

    use super::*;

    async fn client() -> Client {
        client_sending_heartbeats(None).await.0
    }

    async fn client_sending_heartbeats(
        heartbeat_interval: Option<Duration>,
    ) -> (Client, UdpSocket) {
        let device = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)).unwrap();
        device.set_nonblocking(true).unwrap();
        let SocketAddr::V6(device_addr) = device.local_addr().unwrap() else {
            panic!("the device is bound to an IPv6 address");
        };
        let device = tokio::net::UdpSocket::from_std(device).unwrap();
        let answer_every_request = async {
            let unit_info = UnitInfo {
                unit_id: 0,
                flags: Flags::ASSIGNED,
                ptp_offset_ns: I32::new(0),
            };
            let mut buf = [0; 64];
            loop {
                let (_, host) = device.recv_from(&mut buf).await.unwrap();
                let (header, _) = Header::read_from_prefix(&buf).unwrap();
                let reply = [
                    header.as_bytes(),
                    &[Status::Ok.as_u8()],
                    unit_info.as_bytes(),
                ]
                .concat();
                device.send_to(&reply, host).await.unwrap();
            }
        };
        let option = TransportOption {
            iface: Interface::Addr(device_addr),
            open_timeout: Duration::MAX,
            ack_timeout: Duration::from_secs(3600),
            lost_timeout: Duration::from_secs(3600),
            sync_on_open: false,
            heartbeat_interval,
            ..TransportOption::default()
        };
        let geometry = test_geometry(1);
        let client = tokio::select! {
            client = Client::open(&geometry, &option) => client.unwrap(),
            () = answer_every_request => unreachable!(),
        };
        (client, device.into_std().unwrap())
    }

    fn next_request_kind(device: &UdpSocket) -> Option<u8> {
        let mut buf = [0; 64];
        match device.recv(&mut buf) {
            Ok(_) => Some(Header::read_from_prefix(&buf).unwrap().0.kind),
            Err(e) if e.kind() == ErrorKind::WouldBlock => None,
            Err(e) => panic!("{e}"),
        }
    }

    async fn wait_for_a_request(device: &UdpSocket) -> u8 {
        loop {
            if let Some(kind) = next_request_kind(device) {
                return kind;
            }
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test]
    async fn a_heartbeat_interval_keeps_sending_heartbeats() {
        let (client, device) = client_sending_heartbeats(Some(Duration::ZERO)).await;

        assert_eq!(wait_for_a_request(&device).await, Kind::Heartbeat as u8);
        assert_eq!(wait_for_a_request(&device).await, Kind::Heartbeat as u8);

        client.close().await.unwrap();
    }

    #[tokio::test]
    async fn no_heartbeat_interval_sends_no_heartbeat() {
        let (client, device) = client_sending_heartbeats(None).await;

        tokio::task::yield_now().await;

        assert!(client.heartbeat_task.is_none());
        assert_eq!(next_request_kind(&device), None);
    }

    #[tokio::test]
    async fn closing_stops_a_heartbeat_task_waiting_for_its_next_turn() {
        let (client, device) = client_sending_heartbeats(Some(Duration::MAX)).await;
        wait_for_a_request(&device).await;

        client.close().await.unwrap();
        tokio::task::yield_now().await;

        assert!(client.heartbeat_task.as_ref().unwrap().is_finished());
    }

    #[tokio::test]
    async fn dropping_the_client_stops_a_heartbeat_task_waiting_for_its_next_turn() {
        let (client, device) = client_sending_heartbeats(Some(Duration::MAX)).await;
        wait_for_a_request(&device).await;
        let heartbeat_task = client.heartbeat_task.clone().unwrap();

        drop(client);
        tokio::task::yield_now().await;

        assert!(heartbeat_task.is_finished());
    }

    #[tokio::test]
    async fn the_heartbeat_task_ends_once_the_session_has_closed() {
        let (client, device) = client_sending_heartbeats(Some(Duration::ZERO)).await;
        wait_for_a_request(&device).await;

        client.session.close(Error::Closed);

        let heartbeat_task = client.heartbeat_task.as_ref().unwrap();
        while !heartbeat_task.is_finished() {
            tokio::task::yield_now().await;
        }
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
