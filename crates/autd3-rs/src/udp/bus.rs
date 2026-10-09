use std::net::SocketAddrV6;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Flags, FrameReply, Kind, PROTOCOL_VERSION};
use autd3_rs_core::protocol::FrameHeader;
use autd3_rs_core::value::SysTime;
use autd3_rs_core::{BusStats, DeviceClock, FRAME_BYTES_MAX};
use zerocopy::FromBytes;

use super::channel::{Channel, Datagram};
use super::enumerate::{Bringup, bring_up, same_endpoint};
use super::error::UdpError;
use super::iface;
use super::option::TransportOption;
use super::reply::Reply;
use super::state::{SharedState, StateChecker, Tracker};
use super::timer::TimerResolutionGuard;

pub const MAX_DEVICES: usize = 128;

#[derive(Debug)]
pub struct Sent {
    pub msg_id: u16,
    pub devices: u128,
    pub unsent: Option<Unsent>,
}

#[derive(Debug)]
pub struct Unsent {
    pub devices: u128,
    pub device: usize,
    pub cause: std::io::Error,
}

impl From<Unsent> for UdpError {
    fn from(unsent: Unsent) -> Self {
        UdpError::Unsent {
            device: unsent.device,
            source: unsent.cause,
        }
    }
}

enum Incoming {
    Wake,
    Ignored,
    Reply {
        index: usize,
        reply: Reply,
        unit_id: u8,
        flags: Flags,
        sys_time: u64,
    },
}

pub struct UdpBus {
    channel: Channel,
    units: Vec<SocketAddrV6>,
    trackers: Mutex<Vec<Tracker>>,
    shared: Arc<SharedState>,
    stats: BusStats,
    device_clock: DeviceClock,
    heartbeat: Option<Duration>,
    reply_timeout: Duration,
    lost_timeout: Duration,
    msg_id: AtomicU16,
    last_send: Mutex<Instant>,
    closed: AtomicBool,
    _timer_resolution: TimerResolutionGuard,
}

impl UdpBus {
    pub fn open(option: &TransportOption, num_devices: usize) -> Result<Self, UdpError> {
        Self::open_with(option, num_devices, Bringup::Synchronized)
    }

    pub fn open_unsynchronized(
        option: &TransportOption,
        num_devices: usize,
    ) -> Result<Self, UdpError> {
        Self::open_with(option, num_devices, Bringup::Unsynchronized)
    }

    fn open_with(
        option: &TransportOption,
        num_devices: usize,
        bringup: Bringup,
    ) -> Result<Self, UdpError> {
        option.validate()?;
        let timer_resolution = TimerResolutionGuard::new(option.timer_resolution);
        if !(1..=MAX_DEVICES).contains(&num_devices) {
            return Err(UdpError::InvalidDeviceCount(num_devices));
        }
        let target = iface::resolve(&option.iface, option.response_timeout)?;
        let channel = Channel::open(
            target.group,
            target.scope,
            option.send_rate_limit,
            option.send_buffer,
        )?;
        let device_clock = DeviceClock::new();
        let units = bring_up(&channel, option, num_devices, bringup, &device_clock)?;
        Ok(Self {
            channel,
            trackers: Mutex::new(vec![Tracker::new(); units.len()]),
            shared: SharedState::new(units.len()),
            units,
            stats: BusStats::default(),
            device_clock,
            heartbeat: option.heartbeat,
            reply_timeout: option.reply_timeout,
            lost_timeout: option.lost_timeout,
            msg_id: AtomicU16::new(0),
            last_send: Mutex::new(Instant::now()),
            closed: AtomicBool::new(false),
            _timer_resolution: timer_resolution,
        })
    }

    fn trackers(&self) -> MutexGuard<'_, Vec<Tracker>> {
        self.trackers.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn units(&self) -> &[SocketAddrV6] {
        &self.units
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.units.len()
    }

    #[must_use]
    pub fn stats(&self) -> BusStats {
        self.stats.clone()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        StateChecker::new(Arc::clone(&self.shared))
    }

    #[must_use]
    pub fn device_clock(&self) -> DeviceClock {
        self.device_clock.clone()
    }

    #[must_use]
    pub fn heartbeat_interval(&self) -> Option<Duration> {
        self.heartbeat
    }

    #[must_use]
    pub fn reply_timeout(&self) -> Duration {
        self.reply_timeout
    }

    #[must_use]
    pub fn next_msg_id(&self) -> u16 {
        self.msg_id.load(Ordering::Relaxed).wrapping_add(1)
    }

    #[must_use]
    pub fn last_send(&self) -> Instant {
        *self
            .last_send
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    fn ensure_open(&self) -> Result<(), UdpError> {
        if self.is_closed() {
            return Err(UdpError::Closed);
        }
        Ok(())
    }

    #[must_use]
    pub fn reserve_msg_id(&self) -> u16 {
        self.msg_id.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
    }

    pub fn send<F: AsRef<[u8]>>(&self, frames: &[F]) -> Result<Sent, UdpError> {
        self.send_as(self.reserve_msg_id(), frames)
    }

    pub fn send_as<F: AsRef<[u8]>>(&self, msg_id: u16, frames: &[F]) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        if frames.len() != self.units.len() {
            return Err(UdpError::FrameCountMismatch {
                expected: self.units.len(),
                got: frames.len(),
            });
        }
        if let Some(len) = frames
            .iter()
            .map(|frame| frame.as_ref().len())
            .find(|len| !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(len))
        {
            return Err(UdpError::InvalidFrameLength(len));
        }
        self.send_each(Kind::Frame, msg_id, |index| frames[index].as_ref())
    }

    pub fn send_broadcast(&self, frame: &[u8]) -> Result<Sent, UdpError> {
        self.send_broadcast_as(self.reserve_msg_id(), frame)
    }

    pub fn send_broadcast_as(&self, msg_id: u16, frame: &[u8]) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        if !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&frame.len()) {
            return Err(UdpError::InvalidFrameLength(frame.len()));
        }
        self.send_each(Kind::Frame, msg_id, |_| frame)
    }

    pub fn heartbeat(&self) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        let sent = self.send_each(Kind::Heartbeat, self.reserve_msg_id(), |_| &[])?;
        if sent.devices != 0 {
            self.stats.record_heartbeat();
        }
        Ok(sent)
    }

    fn send_each<'a>(
        &self,
        kind: Kind,
        msg_id: u16,
        body: impl Fn(usize) -> &'a [u8],
    ) -> Result<Sent, UdpError> {
        let now = Instant::now();
        let mut give_up = match kind {
            Kind::Frame => now.checked_add(self.lost_timeout),
            _ => None,
        };
        let live = self
            .trackers()
            .iter()
            .enumerate()
            .filter(|(_, tracker)| !tracker.is_lost())
            .fold(0u128, |live, (index, _)| live | 1 << index);
        let mut devices = 0u128;
        let mut unsent: Option<Unsent> = None;
        for index in (0..self.units.len()).rev() {
            if live & (1 << index) == 0 {
                continue;
            }
            let dst = self.units[index];
            let mut attempted_at = None;
            let result = self
                .channel
                .send_with(dst, kind, msg_id, body(index), give_up, || {
                    let now = Instant::now();
                    let mut trackers = self.trackers();
                    if let Some(previous) = attempted_at.replace(now) {
                        trackers[index].unreached(previous);
                    }
                    trackers[index].requested(now);
                });
            if result.is_err()
                && let Some(attempted_at) = attempted_at
            {
                self.trackers()[index].unreached(attempted_at);
            }
            match result {
                Ok(()) => devices |= 1 << index,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if kind == Kind::Frame {
                        if give_up.take().is_some() {
                            tracing::warn!(
                                %dst,
                                "the socket stayed full past the lost timeout; the frame goes unanswered"
                            );
                        }
                        devices |= 1 << index;
                    } else {
                        tracing::trace!(%dst, "the socket is full; skipping the device");
                    }
                }
                Err(e) => {
                    tracing::warn!(%dst, "sending to a device failed; skipping it: {e}");
                    {
                        let mut trackers = self.trackers();
                        trackers[index].send_failed(Instant::now(), self.lost_timeout);
                        self.shared.publish(index, trackers[index].state());
                    }
                    unsent
                        .get_or_insert(Unsent {
                            devices: 0,
                            device: index,
                            cause: e,
                        })
                        .devices |= 1 << index;
                }
            }
        }
        *self
            .last_send
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Instant::now();
        match unsent {
            Some(unsent) if kind == Kind::Frame && devices == 0 => Err(unsent.into()),
            unsent => Ok(Sent {
                msg_id,
                devices,
                unsent,
            }),
        }
    }

    fn parse(&self, datagram: &Datagram<'_>) -> Incoming {
        if self.channel.is_wake(datagram.src) {
            return Incoming::Wake;
        }
        if datagram.header.version != PROTOCOL_VERSION
            || (datagram.header.kind != Kind::Frame.as_u8()
                && datagram.header.kind != Kind::Heartbeat.as_u8())
        {
            return Incoming::Ignored;
        }
        let Ok((head, data)) = FrameReply::read_from_prefix(datagram.body) else {
            return Incoming::Ignored;
        };
        let Some(index) = self
            .units
            .iter()
            .position(|&a| same_endpoint(a, datagram.src))
        else {
            return Incoming::Ignored;
        };
        Incoming::Reply {
            index,
            reply: Reply::new(
                index,
                datagram.header.msg_id.get(),
                head.ack,
                head.status,
                data,
            ),
            unit_id: head.unit_id,
            flags: head.flags,
            sys_time: head.sys_time.get(),
        }
    }

    pub fn recv(&self, deadline: Instant) -> Result<Option<Reply>, UdpError> {
        self.ensure_open()?;
        loop {
            let Some(incoming) = self
                .channel
                .recv(Some(deadline), |datagram| self.parse(datagram))?
            else {
                self.expire(Instant::now());
                return Ok(None);
            };
            match incoming {
                Incoming::Wake => return Ok(None),
                Incoming::Ignored => {}
                Incoming::Reply {
                    index,
                    reply,
                    unit_id,
                    flags,
                    sys_time,
                } => {
                    let accepted = {
                        let mut trackers = self.trackers();
                        let expected = u8::try_from(index).expect("at most 255 units");
                        let accepted = trackers[index].replied(expected, unit_id, flags);
                        self.shared.publish(index, trackers[index].state());
                        accepted
                    };
                    if !accepted {
                        continue;
                    }
                    if index == 0
                        && let Some(sys_time) = NonZeroU64::new(sys_time)
                    {
                        self.device_clock
                            .observe(SysTime::from_nanos(sys_time.get()));
                    }
                    return Ok(Some(reply));
                }
            }
        }
    }

    pub fn wait_readable(&self, deadline: Instant) -> Result<(), UdpError> {
        self.ensure_open()?;
        Ok(self.channel.wait_readable(deadline)?)
    }

    pub fn wake(&self) -> bool {
        match self.channel.wake() {
            Ok(()) => true,
            Err(e) => {
                tracing::debug!("waking the receiver failed: {e}");
                false
            }
        }
    }

    fn expire(&self, now: Instant) {
        let mut trackers = self.trackers();
        for (index, tracker) in trackers.iter_mut().enumerate() {
            tracker.expire(now, self.lost_timeout);
            self.shared.publish(index, tracker.state());
        }
    }

    pub fn close(&self) -> Result<(), UdpError> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.shared.close();
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn fail_sends_to(&self, index: usize, kind: std::io::ErrorKind) {
        self.channel.fail_sends_to(self.units[index], kind);
    }

    #[cfg(test)]
    pub(crate) fn restore_sends(&self) {
        self.channel.restore_sends();
    }
}

#[cfg(unix)]
impl std::os::fd::AsFd for UdpBus {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.channel.socket().as_fd()
    }
}

#[cfg(windows)]
impl std::os::windows::io::AsSocket for UdpBus {
    fn as_socket(&self) -> std::os::windows::io::BorrowedSocket<'_> {
        self.channel.socket().as_socket()
    }
}

impl Drop for UdpBus {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv6Addr, SocketAddr, UdpSocket};

    use autd3_rs_core::DeviceState;
    use autd3_rs_firmware_emulator::udp::UdpEmulator;

    use super::*;

    fn spy() -> (UdpSocket, SocketAddrV6) {
        let socket = UdpSocket::bind((Ipv6Addr::LOCALHOST, 0)).unwrap();
        socket.set_nonblocking(true).unwrap();
        let SocketAddr::V6(addr) = socket.local_addr().unwrap() else {
            unreachable!()
        };
        (socket, addr)
    }

    fn drain(socket: &UdpSocket) -> usize {
        std::thread::sleep(Duration::from_millis(50));
        let mut buf = [0u8; 2048];
        let mut count = 0;
        while socket.recv(&mut buf).is_ok() {
            count += 1;
        }
        count
    }

    fn bus_with(units: Vec<SocketAddrV6>) -> (UdpEmulator, UdpBus) {
        let emulator = UdpEmulator::spawn(units.len()).unwrap();
        let option = TransportOption {
            iface: emulator.interface(),
            response_timeout: Duration::from_millis(50),
            enumeration_timeout: Duration::from_secs(1),
            sync_timeout: Duration::from_secs(5),
            ..TransportOption::default()
        };
        let mut bus = UdpBus::open(&option, units.len()).unwrap();
        bus.units = units;
        (emulator, bus)
    }

    fn make_lost(bus: &UdpBus, index: usize) {
        let unit = u8::try_from(index).unwrap();
        let mut trackers = bus.trackers();
        assert!(!trackers[index].replied(unit, unit, Flags::empty()));
        assert!(trackers[index].is_lost());
    }

    #[test]
    fn a_lost_unit_gets_neither_frames_nor_heartbeats() {
        let (live, live_addr) = spy();
        let (lost, lost_addr) = spy();
        let (_emulator, bus) = bus_with(vec![live_addr, lost_addr]);
        make_lost(&bus, 1);

        assert_eq!(bus.heartbeat().unwrap().devices, 0b01);
        assert_eq!(
            bus.send(&[[0u8; size_of::<FrameHeader>()]; 2])
                .unwrap()
                .devices,
            0b01
        );

        assert_eq!(drain(&live), 2);
        assert_eq!(drain(&lost), 0);
    }

    #[test]
    fn nothing_is_sent_and_nothing_fails_when_every_unit_is_lost() {
        let (lost, lost_addr) = spy();
        let (_emulator, bus) = bus_with(vec![lost_addr]);
        make_lost(&bus, 0);

        assert_eq!(bus.heartbeat().unwrap().devices, 0);
        assert_eq!(
            bus.send(&[[0u8; size_of::<FrameHeader>()]; 1])
                .unwrap()
                .devices,
            0
        );

        assert_eq!(drain(&lost), 0);
    }

    #[test]
    fn waiting_for_readability_returns_on_a_wake_and_leaves_it_for_the_receiver() {
        let (_emulator, bus) = bus_with(vec![spy().1]);
        let start = Instant::now();
        bus.wait_readable(start + Duration::from_millis(20))
            .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(20));

        let start = Instant::now();
        bus.wake();
        bus.wait_readable(start + Duration::from_secs(5)).unwrap();
        bus.wait_readable(start + Duration::from_secs(5)).unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(bus.recv(Instant::now()).unwrap().is_none());
        let start = Instant::now();
        bus.wait_readable(start + Duration::from_millis(20))
            .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn a_wake_returns_the_receiver_without_a_reply() {
        let (_emulator, bus) = bus_with(vec![spy().1]);
        let start = Instant::now();
        bus.wake();
        assert!(bus.recv(start + Duration::from_secs(5)).unwrap().is_none());
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    fn bus_on(emulator: &UdpEmulator) -> UdpBus {
        let option = TransportOption {
            iface: emulator.interface(),
            response_timeout: Duration::from_millis(50),
            enumeration_timeout: Duration::from_secs(1),
            sync_timeout: Duration::from_secs(5),
            ..TransportOption::default()
        };
        UdpBus::open(&option, emulator.num_devices()).unwrap()
    }

    fn replies(bus: &UdpBus, msg_id: u16, count: usize) -> usize {
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut got = 0;
        while got < count {
            match bus.recv(deadline).unwrap() {
                Some(reply) if reply.msg_id == msg_id => got += 1,
                Some(_) => {}
                None => break,
            }
        }
        got
    }

    #[test]
    fn a_frame_the_jammed_socket_never_took_is_not_counted_as_unanswered() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let bus = bus_on(&emulator);
        let checker = bus.state_checker();
        let jammed_until = Instant::now() + bus.lost_timeout * 2;
        bus.channel.jam(jammed_until);

        let sent = bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]).unwrap();
        assert_eq!(sent.devices, 0b11);
        std::thread::sleep(jammed_until.saturating_duration_since(Instant::now()));
        assert!(bus.recv(Instant::now()).unwrap().is_none());
        assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 2]);

        let msg_id = bus.heartbeat().unwrap().msg_id;
        assert_eq!(replies(&bus, msg_id, 2), 2);
        assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 2]);
    }

    #[test]
    fn a_heartbeat_the_jammed_socket_refused_is_not_counted_as_unanswered() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let bus = bus_on(&emulator);
        let checker = bus.state_checker();
        let start = Instant::now();
        bus.channel.jam(start + bus.lost_timeout);

        assert_eq!(bus.heartbeat().unwrap().devices, 0);
        bus.expire(start + bus.lost_timeout * 2);
        assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 2]);
    }

    #[test]
    fn a_unit_reached_late_is_timed_from_its_own_send() {
        let (_first, first_addr) = spy();
        let (_second, second_addr) = spy();
        let (_emulator, bus) = bus_with(vec![first_addr, second_addr]);
        let start = Instant::now();
        bus.channel.jam(start + bus.lost_timeout / 2);

        let sent = bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]).unwrap();
        let returned = Instant::now();
        assert_eq!(sent.devices, 0b11);
        assert!(returned >= start + bus.lost_timeout / 2);

        bus.expire(start + bus.lost_timeout * 5 / 4);
        assert!(bus.trackers().iter().all(|tracker| !tracker.is_lost()));
        bus.expire(returned + bus.lost_timeout);
        assert!(bus.trackers().iter().all(|tracker| tracker.is_lost()));
    }

    #[cfg(target_os = "linux")]
    fn port_zero() -> SocketAddrV6 {
        SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0)
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_send_fails_when_every_unit_it_reaches_fails() {
        let (_lost, lost_addr) = spy();
        let (_emulator, bus) = bus_with(vec![port_zero(), lost_addr]);
        make_lost(&bus, 1);

        let heartbeat = bus.heartbeat().unwrap();
        assert_eq!(heartbeat.devices, 0);
        assert_eq!(heartbeat.unsent.map(|unsent| unsent.devices), Some(0b01));
        assert!(matches!(
            bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]),
            Err(UdpError::Unsent { device: 0, .. })
        ));
    }

    const REFUSED_BY_THE_HOST: std::io::ErrorKind = std::io::ErrorKind::PermissionDenied;

    #[test]
    fn a_frame_one_unit_could_not_be_sent_reports_that_unit() {
        let (reached, reached_addr) = spy();
        let (failing, failing_addr) = spy();
        let (_emulator, bus) = bus_with(vec![reached_addr, failing_addr]);
        bus.fail_sends_to(1, REFUSED_BY_THE_HOST);

        let sent = bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]).unwrap();
        assert_eq!(sent.devices, 0b01);
        let unsent = sent.unsent.unwrap();
        assert_eq!(unsent.devices, 0b10);
        assert_eq!(unsent.device, 1);
        assert_eq!(unsent.cause.kind(), REFUSED_BY_THE_HOST);

        assert_eq!(drain(&reached), 1);
        assert_eq!(drain(&failing), 0);
    }

    #[test]
    fn a_heartbeat_no_unit_could_be_sent_is_not_an_error() {
        let (_first, first_addr) = spy();
        let (_second, second_addr) = spy();
        let (_emulator, bus) = bus_with(vec![first_addr, second_addr]);
        bus.fail_sends_to(0, REFUSED_BY_THE_HOST);
        bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        let heartbeats = bus.stats().heartbeats();

        let sent = bus.heartbeat().unwrap();
        assert_eq!(sent.devices, 0);
        assert_eq!(sent.unsent.map(|unsent| unsent.devices), Some(0b11));
        assert_eq!(bus.stats().heartbeats(), heartbeats);
        assert!(matches!(
            bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]),
            Err(UdpError::Unsent { device: 1, .. })
        ));
    }

    #[test]
    fn a_unit_whose_sends_keep_failing_past_the_lost_timeout_becomes_lost() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let bus = bus_on(&emulator);
        let checker = bus.state_checker();
        bus.fail_sends_to(1, REFUSED_BY_THE_HOST);

        let msg_id = bus.heartbeat().unwrap().msg_id;
        assert_eq!(replies(&bus, msg_id, 1), 1);
        bus.expire(Instant::now() + bus.lost_timeout * 2);
        assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 2]);

        std::thread::sleep(bus.lost_timeout);
        assert_eq!(bus.heartbeat().unwrap().devices, 0b01);
        assert_eq!(
            checker.check().unwrap().devices(),
            [DeviceState::Ready, DeviceState::Lost]
        );
        let sent = bus.heartbeat().unwrap();
        assert_eq!(sent.devices, 0b01);
        assert!(sent.unsent.is_none());
    }

    #[test]
    fn a_unit_that_answers_again_after_a_failed_send_is_not_lost() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let bus = bus_on(&emulator);
        let checker = bus.state_checker();
        bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        bus.heartbeat().unwrap();
        bus.restore_sends();

        let msg_id = bus.heartbeat().unwrap().msg_id;
        assert_eq!(replies(&bus, msg_id, 2), 2);
        std::thread::sleep(bus.lost_timeout);
        bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        bus.heartbeat().unwrap();
        assert_eq!(checker.check().unwrap().devices(), [DeviceState::Ready; 2]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_unit_the_request_never_reached_is_not_counted_as_unanswered() {
        let (_silent, silent_addr) = spy();
        let (_emulator, bus) = bus_with(vec![port_zero(), silent_addr]);

        bus.heartbeat().unwrap();
        bus.send(&[[0u8; size_of::<FrameHeader>()]; 2]).unwrap();
        bus.expire(Instant::now() + bus.lost_timeout * 2);

        assert!(!bus.trackers()[0].is_lost());
        assert!(bus.trackers()[1].is_lost());
    }
}
