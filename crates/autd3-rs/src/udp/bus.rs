use std::net::SocketAddrV6;
use std::sync::Arc;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{FrameReply, Kind, PROTOCOL_VERSION};
use autd3_rs_core::protocol::trimmed_len;
use autd3_rs_core::value::SysTime;
use autd3_rs_core::{BusStats, DeviceClock, FRAME_BYTES_MAX};
use zerocopy::FromBytes;

use super::channel::{Channel, all_nodes};
use super::enumerate::{bring_up, same_endpoint};
use super::error::UdpError;
use super::iface;
use super::option::TransportOption;
use super::reply::Reply;
use super::state::{SharedState, StateChecker, Tracker};
use super::timer::TimerResolutionGuard;

const TIMER_RESOLUTION_MS: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BusTiming {
    pub(crate) heartbeat: Duration,
    pub(crate) reply_timeout: Duration,
}

pub struct UdpBus {
    channel: Channel,
    units: Vec<SocketAddrV6>,
    trackers: Vec<Tracker>,
    shared: Arc<SharedState>,
    stats: BusStats,
    device_clock: DeviceClock,
    timing: BusTiming,
    lost_timeout: Duration,
    msg_id: u16,
    closed: bool,
    _timer_resolution: TimerResolutionGuard,
}

impl UdpBus {
    pub fn open(option: &TransportOption, num_devices: usize) -> Result<Self, UdpError> {
        option.validate()?;
        let timer_resolution = TimerResolutionGuard::new(TIMER_RESOLUTION_MS);
        if !(1..=255).contains(&num_devices) {
            return Err(UdpError::InvalidDeviceCount(num_devices));
        }
        let mut channel = if let Some(group) = option.group {
            Channel::open(group, None)?
        } else {
            let candidate = iface::resolve(&option.iface, option.response_timeout)?;
            Channel::open(all_nodes(candidate.scope), Some(candidate.scope))?
        };
        let units = bring_up(&mut channel, option, num_devices)?;
        Ok(Self {
            channel,
            trackers: vec![Tracker::new(); units.len()],
            shared: SharedState::new(units.len()),
            units,
            stats: BusStats::default(),
            device_clock: DeviceClock::new(),
            timing: BusTiming {
                heartbeat: option.heartbeat,
                reply_timeout: option.reply_timeout,
            },
            lost_timeout: option.lost_timeout,
            msg_id: 0,
            closed: false,
            _timer_resolution: timer_resolution,
        })
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
    pub fn heartbeat_interval(&self) -> Duration {
        self.timing.heartbeat
    }

    #[must_use]
    pub fn reply_timeout(&self) -> Duration {
        self.timing.reply_timeout
    }

    #[must_use]
    pub fn next_msg_id(&self) -> u16 {
        self.msg_id.wrapping_add(1)
    }

    pub(crate) fn timing(&self) -> BusTiming {
        self.timing
    }

    fn ensure_open(&self) -> Result<(), UdpError> {
        if self.closed {
            return Err(UdpError::Closed);
        }
        Ok(())
    }

    pub fn send<F: AsRef<[u8]>>(&mut self, frames: &[F]) -> Result<u16, UdpError> {
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
            .find(|len| !(2..=FRAME_BYTES_MAX).contains(len))
        {
            return Err(UdpError::InvalidFrameLength(len));
        }
        self.msg_id = self.msg_id.wrapping_add(1);
        let msg_id = self.msg_id;
        let mut failure = SendFailure::default();
        for (addr, frame) in self.units.iter().zip(frames).rev() {
            let frame = frame.as_ref();
            failure.record(
                *addr,
                self.channel
                    .send(*addr, Kind::Frame, msg_id, &frame[..trimmed_len(frame)]),
            );
        }
        self.mark_requested();
        failure.into_result(self.units.len())?;
        Ok(msg_id)
    }

    pub fn heartbeat(&mut self) -> Result<u16, UdpError> {
        self.ensure_open()?;
        self.msg_id = self.msg_id.wrapping_add(1);
        let msg_id = self.msg_id;
        let mut failure = SendFailure::default();
        for addr in self.units.iter().rev() {
            failure.record(
                *addr,
                self.channel.send(*addr, Kind::Heartbeat, msg_id, &[]),
            );
        }
        self.mark_requested();
        failure.into_result(self.units.len())?;
        self.stats.record_heartbeat();
        Ok(msg_id)
    }

    pub fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, UdpError> {
        self.ensure_open()?;
        loop {
            let Some(datagram) = self.channel.recv(deadline)? else {
                self.expire(Instant::now());
                return Ok(None);
            };
            if datagram.header.version != PROTOCOL_VERSION
                || (datagram.header.kind != Kind::Frame.as_u8()
                    && datagram.header.kind != Kind::Heartbeat.as_u8())
            {
                continue;
            }
            let Ok((head, data)) = FrameReply::read_from_prefix(datagram.body) else {
                continue;
            };
            let msg_id = datagram.header.msg_id.get();
            let src = datagram.src;
            let Some(index) = self.units.iter().position(|&a| same_endpoint(a, src)) else {
                continue;
            };
            let reply = Reply::new(index, msg_id, head.ack, head.status, head.flags, data);
            let unit = u8::try_from(index).expect("at most 255 units");
            let accepted = self.trackers[index].replied(unit, head.unit_id, head.flags);
            self.shared.publish(index, self.trackers[index].state());
            if !accepted {
                continue;
            }
            if index == 0 {
                let _ = self
                    .device_clock
                    .observe(SysTime::from_nanos(head.sys_time.get()));
            }
            return Ok(Some(reply));
        }
    }

    fn mark_requested(&mut self) {
        let now = Instant::now();
        for tracker in &mut self.trackers {
            tracker.requested(now);
        }
    }

    fn expire(&mut self, now: Instant) {
        for (index, tracker) in self.trackers.iter_mut().enumerate() {
            tracker.expire(now, self.lost_timeout);
            self.shared.publish(index, tracker.state());
        }
    }

    pub fn close(&mut self) -> Result<(), UdpError> {
        if !self.closed {
            self.closed = true;
            self.shared.close();
        }
        Ok(())
    }
}

#[derive(Default)]
struct SendFailure {
    failed: usize,
    first: Option<std::io::Error>,
}

impl SendFailure {
    fn record(&mut self, dst: SocketAddrV6, result: std::io::Result<()>) {
        if let Err(e) = result {
            tracing::warn!(%dst, "sending to a device failed; skipping it: {e}");
            self.failed += 1;
            self.first.get_or_insert(e);
        }
    }

    fn into_result(self, devices: usize) -> std::io::Result<()> {
        match self.first {
            Some(e) if self.failed == devices => Err(e),
            _ => Ok(()),
        }
    }
}

impl Drop for UdpBus {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl crate::transport::Bus for UdpBus {
    type Error = UdpError;

    fn num_devices(&self) -> usize {
        UdpBus::num_devices(self)
    }

    fn stats(&self) -> BusStats {
        UdpBus::stats(self)
    }

    fn device_clock(&self) -> Option<DeviceClock> {
        Some(UdpBus::device_clock(self))
    }

    fn timing(&self) -> BusTiming {
        UdpBus::timing(self)
    }

    fn next_msg_id(&self) -> u16 {
        UdpBus::next_msg_id(self)
    }

    fn send(&mut self, frames: &[[u8; FRAME_BYTES_MAX]]) -> Result<u16, UdpError> {
        UdpBus::send(self, frames)
    }

    fn heartbeat(&mut self) -> Result<u16, UdpError> {
        UdpBus::heartbeat(self)
    }

    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, UdpError> {
        UdpBus::recv(self, deadline)
    }

    fn close(&mut self) -> Result<(), UdpError> {
        UdpBus::close(self)
    }
}
