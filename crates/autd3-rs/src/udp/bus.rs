use std::net::SocketAddrV6;
use std::sync::Arc;
use std::time::{Duration, Instant};

use autd3_cpu_wire::udp::{FrameReply, Kind, PROTOCOL_VERSION};
use autd3_rs_core::value::DcSysTime;
use autd3_rs_core::{BusStats, CycleOutcome, DcClock, RX_FRAME_BYTES, TX_FRAME_BYTES};
use zerocopy::FromBytes;

use super::channel::{Channel, all_nodes};
use super::enumerate::{bring_up, same_endpoint};
use super::error::UdpError;
use super::iface;
use super::option::TransportOption;
use super::state::{SharedState, StateChecker, Tracker};
use super::timer::TimerResolutionGuard;

const TIMER_RESOLUTION_MS: u32 = 1;

pub struct UdpBus {
    channel: Channel,
    units: Vec<SocketAddrV6>,
    trackers: Vec<Tracker>,
    replied: Vec<bool>,
    shared: Arc<SharedState>,
    stats: BusStats,
    dc_clock: DcClock,
    cycle: Duration,
    reply_timeout: Duration,
    next_cycle: Option<Instant>,
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
            replied: vec![false; units.len()],
            shared: SharedState::new(units.len()),
            units,
            stats: BusStats::default(),
            dc_clock: DcClock::new(),
            cycle: option.cycle,
            reply_timeout: option.reply_timeout,
            next_cycle: None,
            msg_id: 0,
            closed: false,
            _timer_resolution: timer_resolution,
        })
    }

    #[must_use]
    pub fn units(&self) -> &[SocketAddrV6] {
        &self.units
    }

    fn unit_of(&self, src: SocketAddrV6) -> Option<usize> {
        self.units.iter().position(|&addr| same_endpoint(addr, src))
    }

    fn accept_reply(&mut self, index: usize, reply: &FrameReply, rx: &mut [[u8; RX_FRAME_BYTES]]) {
        if self.replied[index] {
            return;
        }
        let unit = u8::try_from(index).expect("at most 255 units");
        if !self.trackers[index].replied(unit, reply.unit_id, reply.flags) {
            return;
        }
        self.replied[index] = true;
        rx[index] = [reply.ack, reply.data];
        if index == 0 {
            let _ = self
                .dc_clock
                .observe(DcSysTime::from_nanos(reply.sys_time.get()));
        }
    }
}

impl UdpBus {
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
    pub fn dc_clock(&self) -> DcClock {
        self.dc_clock.clone()
    }

    pub fn wait_next_cycle(&mut self) {
        let now = Instant::now();
        let next = self.next_cycle.unwrap_or(now);
        if next > now {
            std::thread::sleep(next - now);
        }
        let after = next + self.cycle;
        self.next_cycle = Some(if after + self.cycle < Instant::now() {
            Instant::now() + self.cycle
        } else {
            after
        });
    }

    pub fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, UdpError> {
        if self.closed {
            return Err(UdpError::Closed);
        }
        if tx.len() != self.units.len() || rx.len() != self.units.len() {
            return Err(UdpError::FrameCountMismatch {
                expected: self.units.len(),
                tx: tx.len(),
                rx: rx.len(),
            });
        }
        let started = Instant::now();
        self.msg_id = self.msg_id.wrapping_add(1);
        let msg_id = self.msg_id;
        for (addr, frame) in self.units.iter().zip(tx).rev() {
            self.channel.send(*addr, Kind::Frame, msg_id, frame)?;
        }

        self.replied.fill(false);
        let mut remaining = self.units.len();
        let deadline = started + self.reply_timeout;
        while remaining > 0 {
            let Some(datagram) = self.channel.recv(deadline)? else {
                break;
            };
            if datagram.header.version != PROTOCOL_VERSION
                || datagram.header.kind != Kind::Frame.as_u8()
                || datagram.header.msg_id.get() != msg_id
            {
                continue;
            }
            let Ok((reply, _)) = FrameReply::read_from_prefix(datagram.body) else {
                continue;
            };
            let src = datagram.src;
            if let Some(index) = self.unit_of(src) {
                let was = self.replied[index];
                self.accept_reply(index, &reply, rx);
                if !was && self.replied[index] {
                    remaining -= 1;
                }
            }
        }

        for (index, tracker) in self.trackers.iter_mut().enumerate() {
            if !self.replied[index] {
                tracker.missed();
            }
            self.shared.publish(index, tracker.state());
        }

        if remaining == 0 {
            self.stats
                .record_exchange(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
            Ok(CycleOutcome::valid())
        } else {
            self.stats.record_lost_cycle();
            Ok(CycleOutcome::stale())
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

    fn dc_clock(&self) -> Option<DcClock> {
        Some(UdpBus::dc_clock(self))
    }

    fn wait_next_cycle(&mut self) {
        UdpBus::wait_next_cycle(self);
    }

    fn cycle(
        &mut self,
        tx: &[[u8; TX_FRAME_BYTES]],
        rx: &mut [[u8; RX_FRAME_BYTES]],
    ) -> Result<CycleOutcome, UdpError> {
        UdpBus::cycle(self, tx, rx)
    }

    fn close(&mut self) -> Result<(), UdpError> {
        UdpBus::close(self)
    }
}
