use std::net::SocketAddrV6;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{Flags, FrameReply, Kind, PROTOCOL_VERSION};
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::protocol::FrameHeader;
use autd3_rs_core::value::SysTime;
use autd3_rs_core::{BusStats, DeviceClock, FRAME_BYTES_MAX};
use zerocopy::FromBytes;

use super::channel::{Channel, Datagram};
use super::enumerate::{Bringup, Units, bring_up, same_endpoint};
use super::error::UdpError;
use super::iface;
use super::option::TransportOption;
use super::recovery::Recovery;
use super::reply::Reply;
use super::simulator;
use super::state::{SharedState, StateChecker, Tracker};
use super::timer::TimerResolutionGuard;

pub use autd3_rs_core::MAX_DEVICES;

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
    recovery: Mutex<Option<Recovery>>,
    stats: BusStats,
    device_clock: DeviceClock,
    reply_timeout: Duration,
    lost_timeout: Duration,
    msg_id: AtomicU16,
    heartbeat_from: AtomicUsize,
    closed: AtomicBool,
    _timer_resolution: TimerResolutionGuard,
}

impl UdpBus {
    pub fn open_with_geometry(
        option: &TransportOption,
        geometry: &Geometry,
    ) -> Result<Self, UdpError> {
        Self::open_with(
            option,
            Units::Exactly(geometry.num_devices()),
            Bringup::Synchronized,
            Some(geometry),
        )
    }

    pub fn open_unsynchronized(
        option: &TransportOption,
        num_devices: usize,
    ) -> Result<Self, UdpError> {
        Self::open_with(
            option,
            Units::Exactly(num_devices),
            Bringup::Unsynchronized,
            None,
        )
    }

    pub fn open_unsynchronized_all(option: &TransportOption) -> Result<Self, UdpError> {
        Self::open_with(option, Units::All, Bringup::Unsynchronized, None)
    }

    fn open_with(
        option: &TransportOption,
        expected: Units,
        bringup: Bringup,
        geometry: Option<&Geometry>,
    ) -> Result<Self, UdpError> {
        option.validate()?;
        let timer_resolution = TimerResolutionGuard::new(option.timer_resolution);
        if let Units::Exactly(num_devices) = expected
            && !(1..=MAX_DEVICES).contains(&num_devices)
        {
            return Err(UdpError::InvalidDeviceCount(num_devices));
        }
        let target = iface::resolve(&option.iface, option.response_timeout)?;
        if let (Some(control), Some(geometry)) = (target.simulator_control, geometry) {
            simulator::configure(control, geometry, option.enumeration_timeout)?;
        }
        let channel = Channel::open(
            target.group,
            target.scope,
            option.send_rate_limit,
            option.send_buffer,
        )?;
        let device_clock = DeviceClock::new();
        let units = bring_up(&channel, option, expected, bringup, &device_clock)?;
        let last = units.len() - 1;
        Ok(Self {
            channel,
            trackers: Mutex::new(vec![Tracker::new(); units.len()]),
            shared: SharedState::new(units.len()),
            recovery: Mutex::new(None),
            units,
            stats: BusStats::default(),
            device_clock,
            reply_timeout: option.reply_timeout,
            lost_timeout: option.lost_timeout,
            msg_id: AtomicU16::new(0),
            heartbeat_from: AtomicUsize::new(last),
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
    pub fn next_msg_id(&self) -> u16 {
        self.msg_id.load(Ordering::Relaxed).wrapping_add(1)
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

    pub fn send<F: AsRef<[u8]>>(&self, msg_id: u16, frames: &[F]) -> Result<Sent, UdpError> {
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
        self.send_each(Kind::Frame, msg_id, self.units.len() - 1, |index| {
            frames[index].as_ref()
        })
    }

    pub fn broadcast(&self, msg_id: u16, frame: &[u8]) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        if !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&frame.len()) {
            return Err(UdpError::InvalidFrameLength(frame.len()));
        }
        self.send_each(Kind::Frame, msg_id, self.units.len() - 1, |_| frame)
    }

    pub fn heartbeat(&self) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        let sent = self.send_each(
            Kind::Heartbeat,
            self.reserve_msg_id(),
            self.heartbeat_from.load(Ordering::Relaxed),
            |_| &[],
        )?;
        if sent.devices != 0 {
            self.stats.record_heartbeat();
        }
        Ok(sent)
    }

    fn send_each<'a>(
        &self,
        kind: Kind,
        msg_id: u16,
        first: usize,
        body: impl Fn(usize) -> &'a [u8],
    ) -> Result<Sent, UdpError> {
        let now = Instant::now();
        let mut give_up = match kind {
            Kind::Frame => now.checked_add(self.lost_timeout),
            Kind::ReadUnitInfo
            | Kind::ResetId
            | Kind::Discover
            | Kind::AssignId
            | Kind::UnblockDownstream
            | Kind::SetTime
            | Kind::Heartbeat => None,
            _ => unreachable!(),
        };
        let live = self.devices_where(match kind {
            Kind::Frame => Tracker::takes_frames,
            Kind::ReadUnitInfo
            | Kind::ResetId
            | Kind::Discover
            | Kind::AssignId
            | Kind::UnblockDownstream
            | Kind::SetTime
            | Kind::Heartbeat => |tracker: Tracker| !tracker.is_lost(),
            _ => unreachable!(),
        });
        let mut devices = 0u128;
        let mut unsent: Option<Unsent> = None;
        let count = self.units.len();
        for index in (0..count).map(|step| (first + count - step) % count) {
            if live & (1 << index) == 0 {
                continue;
            }
            let dst = self.units[index];
            let mut attempted_at = None;
            let result = self
                .channel
                .send(dst, kind, msg_id, body(index), give_up, || {
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
                        tracing::trace!(
                            %dst,
                            "the socket is full; the next heartbeat resumes from this device"
                        );
                        self.heartbeat_from.store(index, Ordering::Relaxed);
                        break;
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

    fn devices_where(&self, holds: impl Fn(Tracker) -> bool) -> u128 {
        self.trackers()
            .iter()
            .enumerate()
            .filter(|&(_, &tracker)| holds(tracker))
            .fold(0u128, |devices, (index, _)| devices | 1 << index)
    }

    fn recovery(&self) -> MutexGuard<'_, Option<Recovery>> {
        self.recovery.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn probe(&self) -> Result<(), UdpError> {
        self.ensure_open()?;
        let now = Instant::now();
        let lost = self.devices_where(Tracker::is_lost);
        match &mut *self.recovery() {
            Some(recovery) => recovery.extend(now, self.reply_timeout, lost),
            recovery @ None => *recovery = Some(Recovery::new(now, self.reply_timeout, lost)),
        }
        if lost == 0 {
            return Ok(());
        }
        let group = self.channel.group();
        if let Err(e) = self.channel.send(
            group,
            Kind::Heartbeat,
            self.reserve_msg_id(),
            &[],
            None,
            || {},
        ) {
            tracing::debug!(%group, "probing for the lost devices failed: {e}");
        }
        Ok(())
    }

    pub(crate) fn lost_deadline(&self) -> Option<Instant> {
        self.trackers()
            .iter()
            .filter_map(|tracker| tracker.lost_deadline(self.lost_timeout))
            .min()
    }

    #[must_use]
    pub fn recovery_deadline(&self) -> Option<Instant> {
        self.recovery().as_ref().and_then(Recovery::deadline)
    }

    #[must_use]
    pub fn settle_recovery(&self, now: Instant) -> Option<u128> {
        let mut recovery = self.recovery();
        if !recovery.as_ref()?.is_settled(now) {
            return None;
        }
        recovery.take().map(|recovery| recovery.recovered())
    }

    pub fn admit_recovered(&self) {
        for tracker in self.trackers().iter_mut() {
            tracker.admit();
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
                    let (accepted, revived) = {
                        let mut recovery = self.recovery();
                        let mut trackers = self.trackers();
                        let expected =
                            u8::try_from(index).expect("a unit index is below MAX_DEVICES");
                        let accepted = trackers[index].replied(expected, unit_id, flags);
                        let revived = !accepted
                            && recovery.is_some()
                            && trackers[index].revive(expected, unit_id, flags);
                        if revived && let Some(recovery) = &mut *recovery {
                            recovery.restored(index);
                        }
                        self.shared.publish(index, trackers[index].state());
                        (accepted, revived)
                    };
                    if revived {
                        self.stats.record_recovery();
                        tracing::info!(device = index, "a lost device answers again");
                    }
                    if !(accepted || revived) {
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
