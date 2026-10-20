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

use super::channel::{Channel, ReceivedPacket};
use super::enumerate::{ExpectedCount, SyncOnOpen, bring_up, same_endpoint};
use super::error::UdpError;
use super::iface;
use super::option::TransportOption;
use super::recovery::RecoveryWindow;
use super::reply::Reply;
use super::simulator;
use super::state::{DeviceLiveness, PublishedDeviceStates, StateChecker};

pub use autd3_rs_core::MAX_DEVICES;

#[derive(Debug)]
pub struct Sent {
    pub msg_id: u16,
    pub sent_to: u128,
    pub unsent: Option<Unsent>,
}

#[derive(Debug)]
pub struct Unsent {
    pub failed_devices: u128,
    pub first_failed_device: usize,
    pub cause: std::io::Error,
}

impl From<Unsent> for UdpError {
    fn from(unsent: Unsent) -> Self {
        UdpError::Unsent {
            device: unsent.first_failed_device,
            source: unsent.cause,
        }
    }
}

enum ClassifiedPacket {
    WakeSignal,
    Irrelevant,
    DeviceReply {
        reply: Reply,
        reported_unit_id: u8,
        flags: Flags,
        sys_time: u64,
    },
}

pub struct UdpBus {
    channel: Channel,
    device_addrs: Vec<SocketAddrV6>,
    liveness: Mutex<Vec<DeviceLiveness>>,
    published_states: Arc<PublishedDeviceStates>,
    recovery_window: Mutex<Option<RecoveryWindow>>,
    stats: BusStats,
    device_clock: DeviceClock,
    recovery_reply_timeout: Duration,
    lost_timeout: Duration,
    last_msg_id: AtomicU16,
    heartbeat_resume_device: AtomicUsize,
    closed: AtomicBool,
}

impl UdpBus {
    pub fn open_with_geometry(
        option: &TransportOption,
        geometry: &Geometry,
    ) -> Result<Self, UdpError> {
        Self::open_with(
            option,
            ExpectedCount::Exactly(geometry.num_devices()),
            SyncOnOpen::Synchronize,
            Some(geometry),
        )
    }

    pub fn open_unsynchronized(
        option: &TransportOption,
        num_devices: usize,
    ) -> Result<Self, UdpError> {
        Self::open_with(
            option,
            ExpectedCount::Exactly(num_devices),
            SyncOnOpen::Skip,
            None,
        )
    }

    pub fn open_unsynchronized_all(option: &TransportOption) -> Result<Self, UdpError> {
        Self::open_with(option, ExpectedCount::All, SyncOnOpen::Skip, None)
    }

    fn open_with(
        option: &TransportOption,
        expected: ExpectedCount,
        sync_on_open: SyncOnOpen,
        geometry: Option<&Geometry>,
    ) -> Result<Self, UdpError> {
        option.validate()?;
        if let ExpectedCount::Exactly(num_devices) = expected
            && !(1..=MAX_DEVICES).contains(&num_devices)
        {
            return Err(UdpError::InvalidDeviceCount(num_devices));
        }
        let target = iface::resolve(&option.iface, option.control_request_timeout)?;
        if let (Some(control), Some(geometry)) = (target.simulator_control, geometry) {
            simulator::send_geometry(control, geometry, option.enumeration_timeout)?;
        }
        let channel = Channel::open(
            target.all_devices_dst,
            target.scope_id,
            option.send_rate_limit_percent,
            option.send_buffer_bytes,
        )?;
        let device_clock = DeviceClock::new();
        let device_addrs = bring_up(&channel, option, expected, sync_on_open, &device_clock)?;
        let last_device = device_addrs.len() - 1;
        Ok(Self {
            channel,
            liveness: Mutex::new(vec![DeviceLiveness::new(); device_addrs.len()]),
            published_states: PublishedDeviceStates::new(device_addrs.len()),
            recovery_window: Mutex::new(None),
            device_addrs,
            stats: BusStats::default(),
            device_clock,
            recovery_reply_timeout: option.recovery_reply_timeout,
            lost_timeout: option.lost_timeout,
            last_msg_id: AtomicU16::new(0),
            heartbeat_resume_device: AtomicUsize::new(last_device),
            closed: AtomicBool::new(false),
        })
    }

    fn lock_liveness(&self) -> MutexGuard<'_, Vec<DeviceLiveness>> {
        self.liveness.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[must_use]
    pub fn device_addrs(&self) -> &[SocketAddrV6] {
        &self.device_addrs
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.device_addrs.len()
    }

    #[must_use]
    pub fn stats(&self) -> BusStats {
        self.stats.clone()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        StateChecker::new(Arc::clone(&self.published_states))
    }

    #[must_use]
    pub fn device_clock(&self) -> DeviceClock {
        self.device_clock.clone()
    }

    #[must_use]
    pub fn peek_next_msg_id(&self) -> u16 {
        self.last_msg_id.load(Ordering::Relaxed).wrapping_add(1)
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
        self.last_msg_id
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1)
    }

    pub fn send<F: AsRef<[u8]>>(&self, msg_id: u16, frames: &[F]) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        if frames.len() != self.device_addrs.len() {
            return Err(UdpError::FrameCountMismatch {
                expected: self.device_addrs.len(),
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
        self.send_to_each_device(Kind::Frame, msg_id, self.device_addrs.len() - 1, |device| {
            frames[device].as_ref()
        })
    }

    pub fn broadcast(&self, msg_id: u16, frame: &[u8]) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        if !(size_of::<FrameHeader>()..=FRAME_BYTES_MAX).contains(&frame.len()) {
            return Err(UdpError::InvalidFrameLength(frame.len()));
        }
        self.send_to_each_device(Kind::Frame, msg_id, self.device_addrs.len() - 1, |_| frame)
    }

    pub fn send_heartbeat(&self) -> Result<Sent, UdpError> {
        self.ensure_open()?;
        let sent = self.send_to_each_device(
            Kind::Heartbeat,
            self.reserve_msg_id(),
            self.heartbeat_resume_device.load(Ordering::Relaxed),
            |_| &[],
        )?;
        if sent.sent_to != 0 {
            self.stats.record_heartbeat();
        }
        Ok(sent)
    }

    fn send_to_each_device<'a>(
        &self,
        kind: Kind,
        msg_id: u16,
        start_device: usize,
        body_for_device: impl Fn(usize) -> &'a [u8],
    ) -> Result<Sent, UdpError> {
        let now = Instant::now();
        let mut retry_full_socket_until = match kind {
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
        let eligible_devices = self.device_mask_where(match kind {
            Kind::Frame => DeviceLiveness::accepts_frames,
            Kind::ReadUnitInfo
            | Kind::ResetId
            | Kind::Discover
            | Kind::AssignId
            | Kind::UnblockDownstream
            | Kind::SetTime
            | Kind::Heartbeat => |liveness: DeviceLiveness| !liveness.is_lost(),
            _ => unreachable!(),
        });
        let mut accepted_devices = 0u128;
        let mut unsent: Option<Unsent> = None;
        let num_devices = self.device_addrs.len();
        for device in (0..num_devices).map(|step| (start_device + num_devices - step) % num_devices)
        {
            if eligible_devices & (1 << device) == 0 {
                continue;
            }
            let dst = self.device_addrs[device];
            let mut attempted_at = None;
            let result = self.channel.send(
                dst,
                kind,
                msg_id,
                body_for_device(device),
                retry_full_socket_until,
                || {
                    let now = Instant::now();
                    let mut liveness = self.lock_liveness();
                    if let Some(previous) = attempted_at.replace(now) {
                        liveness[device].cancel_unanswered_clock(previous);
                    }
                    liveness[device].start_unanswered_clock(now);
                },
            );
            if result.is_err()
                && let Some(attempted_at) = attempted_at
            {
                self.lock_liveness()[device].cancel_unanswered_clock(attempted_at);
            }
            match result {
                Ok(()) => accepted_devices |= 1 << device,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if kind == Kind::Frame {
                        if retry_full_socket_until.take().is_some() {
                            tracing::warn!(
                                %dst,
                                "the socket stayed full past the lost timeout; the frame goes unanswered"
                            );
                        }
                        accepted_devices |= 1 << device;
                    } else {
                        tracing::trace!(
                            %dst,
                            "the socket is full; the next heartbeat resumes from this device"
                        );
                        self.heartbeat_resume_device
                            .store(device, Ordering::Relaxed);
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!(%dst, "sending to a device failed; skipping it: {e}");
                    {
                        let mut liveness = self.lock_liveness();
                        liveness[device].record_send_failure(Instant::now(), self.lost_timeout);
                        self.published_states
                            .publish(device, liveness[device].state());
                    }
                    unsent
                        .get_or_insert(Unsent {
                            failed_devices: 0,
                            first_failed_device: device,
                            cause: e,
                        })
                        .failed_devices |= 1 << device;
                }
            }
        }
        match unsent {
            Some(unsent) if kind == Kind::Frame && accepted_devices == 0 => Err(unsent.into()),
            unsent => Ok(Sent {
                msg_id,
                sent_to: accepted_devices,
                unsent,
            }),
        }
    }

    fn classify_packet(&self, packet: &ReceivedPacket<'_>) -> ClassifiedPacket {
        if self.channel.is_from_waker(packet.src) {
            return ClassifiedPacket::WakeSignal;
        }
        if packet.header.version != PROTOCOL_VERSION
            || (packet.header.kind != Kind::Frame.as_u8()
                && packet.header.kind != Kind::Heartbeat.as_u8())
        {
            return ClassifiedPacket::Irrelevant;
        }
        let Ok((head, data)) = FrameReply::read_from_prefix(packet.body) else {
            return ClassifiedPacket::Irrelevant;
        };
        let Some(device) = self
            .device_addrs
            .iter()
            .position(|&a| same_endpoint(a, packet.src))
        else {
            return ClassifiedPacket::Irrelevant;
        };
        ClassifiedPacket::DeviceReply {
            reply: Reply::new(
                device,
                packet.header.msg_id.get(),
                head.ack,
                head.status,
                data,
            ),
            reported_unit_id: head.unit_id,
            flags: head.flags,
            sys_time: head.sys_time.get(),
        }
    }

    fn device_mask_where(&self, holds: impl Fn(DeviceLiveness) -> bool) -> u128 {
        self.lock_liveness()
            .iter()
            .enumerate()
            .filter(|&(_, &liveness)| holds(liveness))
            .fold(0u128, |mask, (device, _)| mask | 1 << device)
    }

    fn lock_recovery_window(&self) -> MutexGuard<'_, Option<RecoveryWindow>> {
        self.recovery_window
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub fn start_recovery(&self) -> Result<(), UdpError> {
        self.ensure_open()?;
        let now = Instant::now();
        let lost_devices = self.device_mask_where(DeviceLiveness::is_lost);
        match &mut *self.lock_recovery_window() {
            Some(window) => window.restart_with(now, self.recovery_reply_timeout, lost_devices),
            window @ None => {
                *window = Some(RecoveryWindow::new(
                    now,
                    self.recovery_reply_timeout,
                    lost_devices,
                ));
            }
        }
        if lost_devices == 0 {
            return Ok(());
        }
        let all_devices_dst = self.channel.all_devices_dst();
        if let Err(e) = self.channel.send(
            all_devices_dst,
            Kind::Heartbeat,
            self.reserve_msg_id(),
            &[],
            None,
            || {},
        ) {
            tracing::debug!(%all_devices_dst, "calling the lost devices back failed: {e}");
        }
        Ok(())
    }

    pub(crate) fn lost_deadline(&self) -> Option<Instant> {
        self.lock_liveness()
            .iter()
            .filter_map(|liveness| liveness.lost_deadline(self.lost_timeout))
            .min()
    }

    #[must_use]
    pub fn recovery_deadline(&self) -> Option<Instant> {
        self.lock_recovery_window()
            .as_ref()
            .and_then(RecoveryWindow::deadline)
    }

    #[must_use]
    pub fn take_finished_recovery(&self, now: Instant) -> Option<u128> {
        let mut window = self.lock_recovery_window();
        if !window.as_ref()?.is_finished(now) {
            return None;
        }
        window.take().map(|window| window.recovered_devices())
    }

    pub fn readmit_recovered(&self) {
        for liveness in self.lock_liveness().iter_mut() {
            liveness.readmit();
        }
    }

    pub fn recv(&self, deadline: Instant) -> Result<Option<Reply>, UdpError> {
        self.ensure_open()?;
        loop {
            let Some(classified) = self
                .channel
                .recv(Some(deadline), |packet| self.classify_packet(packet))?
            else {
                self.mark_unanswered_devices_lost(Instant::now());
                return Ok(None);
            };
            match classified {
                ClassifiedPacket::WakeSignal => return Ok(None),
                ClassifiedPacket::Irrelevant => {}
                ClassifiedPacket::DeviceReply {
                    reply,
                    reported_unit_id,
                    flags,
                    sys_time,
                } => {
                    let device = reply.device;
                    let (accepted, revived) = {
                        let mut recovery_window = self.lock_recovery_window();
                        let mut liveness = self.lock_liveness();
                        let expected_unit_id =
                            u8::try_from(device).expect("a device index is below MAX_DEVICES");
                        let accepted = liveness[device].accept_reply(
                            expected_unit_id,
                            reported_unit_id,
                            flags,
                        );
                        let revived = !accepted
                            && recovery_window.is_some()
                            && liveness[device].try_revive(
                                expected_unit_id,
                                reported_unit_id,
                                flags,
                            );
                        if revived && let Some(window) = &mut *recovery_window {
                            window.mark_recovered(device);
                        }
                        self.published_states
                            .publish(device, liveness[device].state());
                        (accepted, revived)
                    };
                    if revived {
                        self.stats.record_recovery();
                        tracing::info!(device, "a lost device answers again");
                    }
                    if !(accepted || revived) {
                        continue;
                    }
                    if device == 0
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

    fn mark_unanswered_devices_lost(&self, now: Instant) {
        let mut liveness = self.lock_liveness();
        for (device, device_liveness) in liveness.iter_mut().enumerate() {
            device_liveness.mark_lost_if_unanswered(now, self.lost_timeout);
            self.published_states
                .publish(device, device_liveness.state());
        }
    }

    pub fn close(&self) -> Result<(), UdpError> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            self.published_states.close();
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
