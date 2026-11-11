mod device_clock;
mod exec_at;
mod inflight;
pub(crate) mod open;

pub use exec_at::ExecAt;
pub use inflight::InflightFrames;

use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Instant;

use autd3_cpu_wire::payload::FirmwareInfo;
use autd3_cpu_wire::udp::{Flags, Kind};
use autd3_cpu_wire::update::RunningImage;
use autd3_rs_core::MAX_DEVICES;
use zerocopy::{FromBytes, FromZeros, IntoBytes};

use crate::commands::Command;
use crate::commands::operation::Distribution;
use crate::datagram::{Datagram, Frame, Frames};
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::geometry::Geometry;
use crate::protocol::{Cmd, DeviceErrorCode, FrameHeader, PAYLOAD_BYTES};
use crate::response::Response;
use crate::udp::{Channel, DeviceReply, TransportOption, UdpError};
use crate::value::SysTime;

use device_clock::DeviceClock;

const GRANDMASTER_UNIT_ID: usize = 0;

pub struct Controller {
    geometry: Geometry,
    channel: Channel,
    last_msg_id: AtomicU16,
    device_clock: Mutex<DeviceClock>,
}

impl Controller {
    pub async fn open(geometry: &Geometry, option: &TransportOption) -> Result<Self, Error> {
        let mut device_clock = DeviceClock::default();
        let channel = open::open(option, geometry.num_devices(), &mut device_clock).await?;
        tracing::info!(num_devices = geometry.num_devices(), "controller opened");
        Ok(Self {
            geometry: geometry.clone(),
            channel,
            last_msg_id: AtomicU16::new(0),
            device_clock: Mutex::new(device_clock),
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

    fn device_clock(&self) -> std::sync::MutexGuard<'_, DeviceClock> {
        self.device_clock
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.device_time_at(Instant::now())
    }

    pub fn device_time_at(&self, host: Instant) -> Result<SysTime, Error> {
        self.device_clock().at(host).ok_or(Error::DeviceTimeUnknown)
    }

    pub fn discard_unread_replies(&self) -> Result<(), Error> {
        while let Some((reply, received_at)) = self
            .channel
            .try_recv_from_device()
            .map_err(UdpError::from)?
        {
            self.check_and_observe(&reply, received_at).ok();
        }
        Ok(())
    }

    fn check_and_observe(&self, reply: &DeviceReply, received_at: Instant) -> Result<(), Error> {
        if !reply.flags.contains(Flags::ASSIGNED) || usize::from(reply.unit_id) != reply.device {
            return Err(Error::DeviceLost {
                device: reply.device,
            });
        }
        if reply.device == GRANDMASTER_UNIT_ID {
            let mut device_clock = self.device_clock();
            if reply.sys_time != 0 {
                device_clock.add_lower_bound(SysTime::from_nanos(reply.sys_time), received_at);
            }
            if reply.rx_sys_time != 0 {
                device_clock
                    .add_upper_bound_of(reply.msg_id, SysTime::from_nanos(reply.rx_sys_time));
            }
        }
        Ok(())
    }

    async fn send_and_collect(&self, frame: Frame<'_>) -> Result<Response, Error> {
        self.discard_unread_replies()?;
        let msg_id = self.send_frame(frame, ExecAt::Immediate).await?;
        self.collect(msg_id).await
    }

    pub fn send<C: Command>(&self, cmd: C) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        let frames = Frames::encode(&self.geometry, cmd);
        async move {
            for frame in &frames? {
                self.send_and_collect(frame).await?.check()?;
            }
            Ok(())
        }
    }

    async fn send_to_each_device<'a>(
        &self,
        kind: Kind,
        body_for_device: impl Fn(usize) -> [&'a [u8]; 2],
    ) -> Result<u16, Error> {
        let msg_id = self
            .last_msg_id
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        let mut first_failure = None;
        for device in (0..self.channel.num_devices()).rev() {
            if device == GRANDMASTER_UNIT_ID {
                self.device_clock()
                    .record_sent_to_grandmaster(msg_id, Instant::now());
            }
            if let Err(source) = self
                .channel
                .send_to_device(device, kind, msg_id, body_for_device(device))
                .await
            {
                tracing::warn!(device, "sending to a device failed; skipping it: {source}");
                first_failure.get_or_insert(UdpError::Unsent { device, source });
            }
        }
        first_failure.map_or(Ok(msg_id), |e| Err(e.into()))
    }

    pub async fn send_frame(&self, frame: Frame<'_>, exec: ExecAt) -> Result<u16, Error> {
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
        let now = match exec {
            ExecAt::Immediate => None,
            ExecAt::At(_) => self.device_clock().at(Instant::now()),
        };
        let exec_at = exec_at::schedule(exec, now)?;
        let mut headers = [FrameHeader::new_zeroed(); MAX_DEVICES];
        for (header, datagram) in headers.iter_mut().zip(datagrams) {
            *header = FrameHeader {
                reserved: 0,
                cmd: datagram.cmd.as_u8(),
                exec_at: exec_at.into(),
            };
        }
        tracing::trace!(cmd = ?datagrams[0].cmd, dist = ?frame.distribution(), "sending frame");
        self.send_to_each_device(Kind::Frame, |device| {
            let index = frame.datagram_index(device);
            [headers[index].as_bytes(), datagrams[index].payload()]
        })
        .await
    }

    pub async fn send_heartbeat(&self) -> Result<u16, Error> {
        self.send_to_each_device(Kind::Heartbeat, |_| [&[][..]; 2])
            .await
    }

    pub async fn recv(&self) -> Result<DeviceReply, Error> {
        let (reply, received_at) = self
            .channel
            .recv_from_device()
            .await
            .map_err(UdpError::from)?;
        self.check_and_observe(&reply, received_at)?;
        Ok(reply)
    }

    pub async fn collect(&self, msg_id: u16) -> Result<Response, Error> {
        let mut response = Response::for_devices(self.num_devices());
        while !response.has_all_replies() {
            let reply = self.recv().await?;
            if reply.msg_id == msg_id {
                response.record_reply(reply);
            }
        }
        Ok(response)
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let datagram = [Datagram::no_payload(Cmd::ReadFirmwareInfo)];
        let response = self
            .send_and_collect(Frame::new(Distribution::Broadcast, &datagram))
            .await?;
        response.check()?;
        response.parse_each_device(|value| {
            FirmwareInfo::read_from_prefix(value)
                .ok()
                .map(|(info, _)| FirmwareVersion::from_info(info))
        })
    }

    #[doc(hidden)]
    pub async fn read_running_image(&self) -> Result<Vec<Option<RunningImage>>, Error> {
        let datagram = [Datagram::no_payload(Cmd::ReadRunningImage)];
        let response = self
            .send_and_collect(Frame::new(Distribution::Broadcast, &datagram))
            .await?;
        response
            .replies()
            .iter()
            .map(|reply| match DeviceErrorCode::from_u8(reply.status) {
                Some(DeviceErrorCode::UnknownCmd) => Ok(None),
                Some(DeviceErrorCode::None) => reply
                    .data
                    .first()
                    .map(|&raw| Some(RunningImage::from_u8(raw).unwrap_or(RunningImage::Unknown)))
                    .ok_or(Error::UnexpectedReply {
                        device: reply.device,
                    }),
                _ => Err(Error::DeviceError {
                    device: reply.device,
                    code: reply.status,
                }),
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6};

    use autd3_cpu_wire::udp::{FrameReply, Header, Status, UnitInfo};
    use autd3_rs_core::Interface;
    use tokio::net::UdpSocket;
    use zerocopy::little_endian::{I32, U64};

    use crate::commands::Nop;
    use crate::fpga_state::FpgaState;
    use crate::telemetry::Telemetry;
    use crate::test_utils::test_geometry;

    use super::*;

    pub(crate) async fn open_controller() -> (Controller, UdpSocket) {
        let device = UdpSocket::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0))
            .await
            .unwrap();
        let SocketAddr::V6(device_addr) = device.local_addr().unwrap() else {
            panic!("the device is bound to an IPv6 address");
        };
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
            sync_on_open: false,
            ..TransportOption::default()
        };
        let geometry = test_geometry(1);
        let controller = tokio::select! {
            controller = Controller::open(&geometry, &option) => controller.unwrap(),
            () = answer_every_request => unreachable!(),
        };
        (controller, device)
    }

    pub(crate) async fn next_request(device: &UdpSocket) -> (Header, SocketAddr) {
        let mut buf = [0; 2048];
        let (_, host) = device.recv_from(&mut buf).await.unwrap();
        (Header::read_from_prefix(&buf).unwrap().0, host)
    }

    pub(crate) async fn reply(
        device: &UdpSocket,
        host: SocketAddr,
        msg_id: u16,
        status: u8,
        flags: Flags,
        data: &[u8],
    ) {
        let head = FrameReply {
            reserved: 0,
            status,
            flags,
            unit_id: 0,
            sys_time: U64::new(0),
            rx_sys_time: U64::new(0),
            telemetry: autd3_cpu_wire::payload::Telemetry {
                exec_late: 6.into(),
                ..Default::default()
            },
            fpga_state: 0x10,
        };
        let datagram = [
            Header::new(Kind::Frame, msg_id).as_bytes(),
            head.as_bytes(),
            data,
        ]
        .concat();
        device.send_to(&datagram, host).await.unwrap();
    }

    pub(crate) async fn answer_one_frame(device: &UdpSocket, status: u8) {
        let (header, host) = next_request(device).await;
        reply(
            device,
            host,
            header.msg_id.get(),
            status,
            Flags::ASSIGNED,
            &[],
        )
        .await;
    }

    #[tokio::test]
    async fn send_waits_for_the_reply_of_every_frame() {
        let (controller, device) = open_controller().await;

        let (sent, ()) = tokio::join!(controller.send(Nop), answer_one_frame(&device, 0));

        sent.unwrap();
    }

    #[cfg(not(target_os = "macos"))]
    #[tokio::test]
    async fn send_fails_once_the_device_port_is_closed() {
        let (controller, device) = open_controller().await;
        drop(device);

        let Err(Error::Network(cause)) = controller.send(Nop).await else {
            panic!("a send to a closed port fails with a network error");
        };
        let UdpError::Io(e) = &cause else {
            panic!("the network error carries the socket error");
        };
        assert!(matches!(
            e.kind(),
            std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::ConnectionReset
        ));
    }

    #[tokio::test]
    async fn send_reports_the_error_code_a_device_replied() {
        let (controller, device) = open_controller().await;

        let (sent, ()) = tokio::join!(controller.send(Nop), answer_one_frame(&device, 0x07));

        assert!(matches!(
            sent,
            Err(Error::DeviceError {
                device: 0,
                code: 0x07
            })
        ));
    }

    #[tokio::test]
    async fn each_send_gets_the_next_msg_id() {
        let (controller, device) = open_controller().await;
        let datagram = [Datagram::no_payload(Cmd::Nop)];
        let frame = || Frame::new(Distribution::Broadcast, &datagram);

        let first = controller
            .send_frame(frame(), ExecAt::Immediate)
            .await
            .unwrap();
        let heartbeat = controller.send_heartbeat().await.unwrap();
        let second = controller
            .send_frame(frame(), ExecAt::Immediate)
            .await
            .unwrap();

        assert_eq!(heartbeat, first.wrapping_add(1));
        assert_eq!(second, first.wrapping_add(2));
        for (kind, msg_id) in [
            (Kind::Frame, first),
            (Kind::Heartbeat, heartbeat),
            (Kind::Frame, second),
        ] {
            let (header, _) = next_request(&device).await;
            assert_eq!((header.kind, header.msg_id.get()), (kind.as_u8(), msg_id));
        }
    }

    #[tokio::test]
    async fn inflight_frames_keep_replies_that_overtake_the_oldest() {
        let (controller, device) = open_controller().await;
        let datagram = [Datagram::no_payload(Cmd::Nop)];
        let mut inflight = InflightFrames::default();
        for _ in 0..2 {
            let msg_id = controller
                .send_frame(
                    Frame::new(Distribution::Broadcast, &datagram),
                    ExecAt::Immediate,
                )
                .await
                .unwrap();
            inflight.push(msg_id);
        }
        let (first, host) = next_request(&device).await;
        let (second, _) = next_request(&device).await;
        reply(&device, host, 0xFFFF, 0x07, Flags::ASSIGNED, &[]).await;
        reply(
            &device,
            host,
            second.msg_id.get(),
            0x02,
            Flags::ASSIGNED,
            &[],
        )
        .await;
        reply(
            &device,
            host,
            second.msg_id.get(),
            0x03,
            Flags::ASSIGNED,
            &[],
        )
        .await;
        reply(
            &device,
            host,
            first.msg_id.get(),
            0x01,
            Flags::ASSIGNED,
            &[],
        )
        .await;

        let oldest = inflight.collect_oldest(&controller).await.unwrap().unwrap();
        let next = tokio::select! {
            biased;
            next = inflight.collect_oldest(&controller) => next.unwrap().unwrap(),
            () = std::future::ready(()) => panic!("the overtaking reply was dropped"),
        };

        assert_eq!(oldest.replies()[0].status, 0x01);
        assert_eq!(next.replies()[0].status, 0x02);
        assert!(inflight.is_empty());
        assert_eq!(inflight.collect_oldest(&controller).await.unwrap(), None);
    }

    #[tokio::test]
    async fn collect_skips_replies_to_other_messages() {
        let (controller, device) = open_controller().await;
        let datagram = [Datagram::no_payload(Cmd::Nop)];
        let msg_id = controller
            .send_frame(
                Frame::new(Distribution::Broadcast, &datagram),
                ExecAt::Immediate,
            )
            .await
            .unwrap();
        let (_, host) = next_request(&device).await;
        reply(
            &device,
            host,
            msg_id.wrapping_add(1),
            0x01,
            Flags::ASSIGNED,
            &[],
        )
        .await;
        reply(&device, host, msg_id, 0x02, Flags::ASSIGNED, &[9]).await;

        let response = controller.collect(msg_id).await.unwrap();

        assert_eq!(response.replies()[0].status, 0x02);
        assert_eq!(response.replies()[0].data, [9]);
    }

    #[tokio::test]
    async fn recv_returns_each_reply_as_it_is() {
        let (controller, device) = open_controller().await;
        let msg_id = controller.send_heartbeat().await.unwrap();
        let (_, host) = next_request(&device).await;
        let flags = Flags::ASSIGNED | Flags::FAILSAFE;
        reply(&device, host, msg_id, 0x03, flags, &[1, 2]).await;

        let received = controller.recv().await.unwrap();

        assert_eq!(
            received,
            DeviceReply {
                device: 0,
                msg_id,
                status: 0x03,
                flags,
                unit_id: 0,
                sys_time: 0,
                rx_sys_time: 0,
                telemetry: Telemetry {
                    exec_late: 6,
                    ..Telemetry::default()
                },
                fpga_state: FpgaState(0x10),
                data: vec![1, 2],
            }
        );
    }

    #[tokio::test]
    async fn discarding_unread_replies_observes_the_grandmaster_time() {
        let (controller, device) = open_controller().await;
        let msg_id = controller.send_heartbeat().await.unwrap();
        let (_, host) = next_request(&device).await;
        let head = FrameReply {
            reserved: 0,
            status: 0,
            flags: Flags::ASSIGNED,
            unit_id: 0,
            sys_time: U64::new(7_000_000_000),
            rx_sys_time: U64::new(6_999_000_000),
            telemetry: autd3_cpu_wire::payload::Telemetry::default(),
            fpga_state: 0,
        };
        let datagram = [
            Header::new(Kind::Heartbeat, msg_id).as_bytes(),
            head.as_bytes(),
        ]
        .concat();
        device.send_to(&datagram, host).await.unwrap();
        controller.channel.readable().await.unwrap();
        assert!(matches!(
            controller.device_time_now(),
            Err(Error::DeviceTimeUnknown)
        ));

        controller.discard_unread_replies().unwrap();

        assert!(controller.device_time_now().unwrap() >= SysTime::from_nanos(6_999_000_000));
        assert_eq!(controller.channel.try_recv_from_device().unwrap(), None);
    }

    #[tokio::test]
    async fn a_grandmaster_reply_to_a_recorded_send_bounds_the_offset_from_above() {
        let (controller, device) = open_controller().await;
        for (rx_sys_time, upper_bounds) in [(0, 0), (6_999_000_000, 1)] {
            let msg_id = controller.send_heartbeat().await.unwrap();
            let (_, host) = next_request(&device).await;
            let head = FrameReply {
                reserved: 0,
                status: 0,
                flags: Flags::ASSIGNED,
                unit_id: 0,
                sys_time: U64::new(7_000_000_000),
                rx_sys_time: U64::new(rx_sys_time),
                telemetry: autd3_cpu_wire::payload::Telemetry::default(),
                fpga_state: 0,
            };
            let datagram = [
                Header::new(Kind::Heartbeat, msg_id).as_bytes(),
                head.as_bytes(),
            ]
            .concat();
            device.send_to(&datagram, host).await.unwrap();
            controller.recv().await.unwrap();
            assert_eq!(
                controller.device_clock().upper_bound_buckets(),
                upper_bounds
            );
        }
    }

    #[tokio::test]
    async fn a_reply_carrying_another_unit_id_reports_the_device_as_lost() {
        let (controller, device) = open_controller().await;
        let msg_id = controller.send_heartbeat().await.unwrap();
        let (_, host) = next_request(&device).await;
        let head = FrameReply {
            reserved: 0,
            status: 0,
            flags: Flags::ASSIGNED,
            unit_id: 1,
            sys_time: U64::new(0),
            rx_sys_time: U64::new(0),
            telemetry: autd3_cpu_wire::payload::Telemetry::default(),
            fpga_state: 0,
        };
        let datagram = [
            Header::new(Kind::Heartbeat, msg_id).as_bytes(),
            head.as_bytes(),
        ]
        .concat();
        device.send_to(&datagram, host).await.unwrap();

        assert!(matches!(
            controller.recv().await,
            Err(Error::DeviceLost { device: 0 })
        ));
    }

    #[tokio::test]
    async fn an_unassigned_reply_reports_the_device_as_lost() {
        let (controller, device) = open_controller().await;
        let msg_id = controller.send_heartbeat().await.unwrap();
        let (_, host) = next_request(&device).await;
        reply(&device, host, msg_id, 0, Flags::empty(), &[]).await;

        assert!(matches!(
            controller.collect(msg_id).await,
            Err(Error::DeviceLost { device: 0 })
        ));
    }
}
