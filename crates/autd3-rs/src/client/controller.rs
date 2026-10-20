use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

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
use crate::protocol::{Cmd, PAYLOAD_BYTES, Seq};
use crate::telemetry::Telemetry;
use crate::udp::frame_buf::FrameBuf;
use crate::udp::{Sent, StateChecker, TransportOption, UdpBus, UdpError};
use crate::value::{Intensity, SysTime};
use autd3_rs_core::{BusStats, DeviceClock, DeviceMask};
use futures_channel::oneshot;

use super::MAX_DEVICES;
use super::completion::{self, ResponseFuture};
use super::driver::{Driver, InflightRequest, SendOutcome, SharedSession, devices_sent_to};
use super::exec_at::{self, ExecAt};
use super::pool::{ReplySlot, ReplySlotPool};

pub struct Controller {
    geometry: Arc<Geometry>,
    bus: Arc<UdpBus>,
    session: Arc<SharedSession>,
    reply_slots: Arc<ReplySlotPool>,
    send_serializer: AsyncMutex<()>,
    tx_frame_bufs: Mutex<Vec<FrameBuf>>,
    lost_timeout: Duration,
    silent_stop_sent: AsyncMutex<bool>,
    device_clock: DeviceClock,
    stats: BusStats,
}

impl Controller {
    pub fn open(geometry: &Geometry, option: &TransportOption) -> Result<(Self, Driver), Error> {
        let lost_timeout = option.lost_timeout;
        let num_devices = geometry.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        let bus = Arc::new(UdpBus::open_with_geometry(option, geometry)?);
        let session = SharedSession::new(option.ack_timeout);
        let driver = Driver::new(Arc::clone(&bus), Arc::clone(&session));
        let controller = Self {
            geometry: Arc::new(geometry.clone()),
            reply_slots: ReplySlotPool::new(num_devices),
            send_serializer: AsyncMutex::new(()),
            tx_frame_bufs: Mutex::new(vec![FrameBuf::new(); num_devices]),
            lost_timeout,
            silent_stop_sent: AsyncMutex::new(false),
            device_clock: bus.device_clock(),
            stats: bus.stats(),
            session,
            bus,
        };
        Ok((controller, driver))
    }

    pub async fn initialize(&self) -> Result<(), Error> {
        let _serialized = self.send_serializer.lock().await;
        self.reset().await?;
        tracing::info!(num_devices = self.num_devices(), "handshake complete");
        Ok(())
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
        self.stats.clone()
    }

    #[must_use]
    pub fn state_checker(&self) -> StateChecker {
        self.bus.state_checker()
    }

    pub fn device_time_now(&self) -> Result<SysTime, Error> {
        self.device_clock.now().ok_or(Error::DeviceTimeUnknown)
    }

    pub fn send<'a, C: Command<'a>>(
        &self,
        cmd: C,
    ) -> impl Future<Output = Result<(), Error>> + use<'_, C> {
        let frames = Frames::encode(&self.geometry, cmd);
        async move {
            for frame in &frames? {
                self.send_frame(frame).await?.await?.check()?;
            }
            Ok(())
        }
    }

    pub async fn send_frame(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        self.validate_and_send(frame.distribution(), frame.datagrams(), ExecAt::Immediate)
            .await
    }

    pub async fn send_frame_at(
        &self,
        frame: Frame<'_>,
        exec: ExecAt,
    ) -> Result<ResponseFuture, Error> {
        self.validate_and_send(frame.distribution(), frame.datagrams(), exec)
            .await
    }

    fn closed_error(&self) -> Option<Error> {
        if self.session.is_stopping() {
            return Some(
                self.session
                    .lock_state()
                    .closed_error()
                    .unwrap_or(Error::Closed),
            );
        }
        self.session.lock_state().closed_error()
    }

    async fn validate_and_send(
        &self,
        dist: Distribution,
        datagrams: &[Datagram],
        exec: ExecAt,
    ) -> Result<ResponseFuture, Error> {
        let expected = match dist {
            Distribution::Broadcast => 1,
            Distribution::PerDevice => self.num_devices(),
        };
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
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let reply_slot = self.reply_slots.acquire().await;
        if self.session.lock_state().reset_required {
            self.reset().await?;
        }
        let exec_at = exec_at::schedule(exec, self.lost_timeout, || self.device_clock.now())?;
        self.send_with_next_seq(reply_slot, dist, datagrams, exec_at)
    }

    fn send_with_next_seq(
        &self,
        mut reply_slot: ReplySlot,
        dist: Distribution,
        datagrams: &[Datagram],
        exec_at: u32,
    ) -> Result<ResponseFuture, Error> {
        reply_slot.reset();
        let seq = {
            let mut state = self.session.lock_state();
            let seq = state.next_seq;
            state.next_seq = seq.next();
            seq
        };
        tracing::trace!(seq = seq.get(), cmd = ?datagrams[0].cmd, ?dist, "sending frame");
        let (tx, rx) = completion::channel();
        let msg_id = self.bus.reserve_msg_id();
        self.session.lock_state().push_awaiting(InflightRequest {
            msg_id,
            seq: Some(seq),
            sent_to: None,
            replied_devices: 0,
            sent_at: Instant::now(),
            reply_slot: Some(reply_slot),
            completion: tx,
        });
        let sent = {
            let mut bufs = self
                .tx_frame_bufs
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            match dist {
                Distribution::Broadcast => {
                    bufs[0].write_frame(seq, datagrams[0].cmd, exec_at, datagrams[0].payload());
                    self.bus.broadcast(msg_id, bufs[0].as_ref())
                }
                Distribution::PerDevice => {
                    for (buf, datagram) in bufs.iter_mut().zip(datagrams) {
                        buf.write_frame(seq, datagram.cmd, exec_at, datagram.payload());
                    }
                    self.bus.send(msg_id, &bufs)
                }
            }
        };
        if self.apply_send_result(msg_id, sent)? {
            self.stats.record_frame();
        }
        Ok(rx)
    }

    fn apply_send_result(&self, msg_id: u16, sent: Result<Sent, UdpError>) -> Result<bool, Error> {
        let (outcome, driver_waits_too_long, ack_timeout) = {
            let mut state = self.session.lock_state();
            let ack_timeout = state.ack_timeout;
            let outcome = state.apply_send_result(msg_id, devices_sent_to(sent));
            let driver_waits_too_long = matches!(outcome, SendOutcome::SentToSome(_))
                && state.ack_deadline_precedes_driver_wake(msg_id, ack_timeout);
            (outcome, driver_waits_too_long, ack_timeout)
        };
        match outcome {
            SendOutcome::Failed(request, e) => {
                drop(request);
                return Err(e);
            }
            SendOutcome::SentToNone(request) => {
                if let Some(request) = request {
                    request.completion.send(Err(Error::Timeout {
                        timeout: ack_timeout,
                    }));
                }
                return Ok(false);
            }
            SendOutcome::SentToSome(answered) => {
                if let Some(request) = answered {
                    request.complete(&self.stats);
                }
            }
        }
        if driver_waits_too_long && !self.bus.wake() {
            tracing::debug!(
                msg_id,
                "could not wake the driver; the ack timeout is noticed at its next deadline"
            );
        }
        Ok(true)
    }

    pub fn send_heartbeat(&self) -> Result<(), Error> {
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        if let Err(e) = self.bus.send_heartbeat() {
            return Err(match e {
                UdpError::Closed => self.closed_error().unwrap_or(Error::Closed),
                e => e.into(),
            });
        }
        let driver_waits_too_long = self.bus.lost_deadline().is_some_and(|deadline| {
            self.session
                .lock_state()
                .driver_wakes_at
                .is_none_or(|wakes_at| deadline < wakes_at)
        });
        if driver_waits_too_long && !self.bus.wake() {
            tracing::debug!(
                "could not wake the driver; a silent device is noticed at its next deadline"
            );
        }
        Ok(())
    }

    pub async fn recover(&self) -> Result<DeviceMask, Error> {
        let _serialized = self.send_serializer.lock().await;
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let (tx, rx) = oneshot::channel();
        self.session.lock_state().register_recovery_waiter(tx)?;
        if let Err(e) = self.bus.start_recovery() {
            self.session.lock_state().recovery_waiter = None;
            return Err(e.into());
        }
        if !self.bus.wake() {
            tracing::debug!(
                "could not wake the driver; the recovery finishes at its next deadline"
            );
        }
        let recovered_devices = rx.await.unwrap_or(Err(Error::Closed))?;
        tracing::debug!(recovered_devices, "recovery finished");
        if recovered_devices != 0 {
            self.reset().await?;
        }
        Ok(DeviceMask::from_bits(recovered_devices))
    }

    async fn reset(&self) -> Result<(), Error> {
        self.bus.readmit_recovered();
        let stale_requests = self.session.lock_state().take_all_awaiting();
        if !stale_requests.is_empty() {
            tracing::debug!(
                stale = stale_requests.len(),
                "failing the frames left in flight before the reset"
            );
            let timeout = self.session.lock_state().ack_timeout;
            for request in stale_requests {
                request.completion.send(Err(Error::Timeout { timeout }));
            }
        }
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        tracing::debug!("sending reset");
        let (tx, rx) = completion::channel();
        let msg_id = self.bus.reserve_msg_id();
        self.session.lock_state().push_awaiting(InflightRequest {
            msg_id,
            seq: None,
            sent_to: None,
            replied_devices: 0,
            sent_at: Instant::now(),
            reply_slot: None,
            completion: tx,
        });
        let sent = {
            let mut bufs = self
                .tx_frame_bufs
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            bufs[0].write_frame(Seq::ZERO, Cmd::Reset, 0, &[]);
            self.bus.broadcast(msg_id, bufs[0].as_ref())
        };
        let reached = self.apply_send_result(msg_id, sent)?;
        self.stats.record_reset();
        if !reached {
            return Err(Error::Timeout {
                timeout: self.session.lock_state().ack_timeout,
            });
        }
        if let Err(e) = rx.await {
            if matches!(e, Error::Timeout { .. }) {
                tracing::warn!("the devices did not acknowledge the reset");
            }
            return Err(e);
        }
        let mut state = self.session.lock_state();
        state.next_seq = Seq::ZERO;
        state.reset_required = false;
        tracing::debug!("reset confirmed by every device");
        Ok(())
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
        let response = self
            .validate_and_send(
                Distribution::Broadcast,
                &[Datagram::no_payload(cmd)],
                ExecAt::Immediate,
            )
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
        let mut silent_stop_sent = self.silent_stop_sent.lock().await;
        if !*silent_stop_sent {
            self.silent_stop().await?;
            *silent_stop_sent = true;
        }
        self.shutdown();
        Ok(())
    }

    pub(crate) fn shutdown(&self) {
        self.session.request_stop();
        if !self.bus.wake() {
            tracing::warn!("could not wake the driver; it stops at its next deadline");
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}
