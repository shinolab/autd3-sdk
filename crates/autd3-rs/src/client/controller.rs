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

use super::completion::{self, ResponseFuture};
use super::config::{ClientConfig, MAX_DEVICES};
use super::driver::{Driver, Inflight, Settlement, Shared, sent_devices};
use super::exec_at::{self, ExecAt};
use super::pool::{Slot, SlotPool};

pub struct Controller {
    geometry: Arc<Geometry>,
    bus: Arc<UdpBus>,
    shared: Arc<Shared>,
    pool: Arc<SlotPool>,
    gate: AsyncMutex<()>,
    bufs: Mutex<Vec<FrameBuf>>,
    lost_timeout: Duration,
    stopped: AsyncMutex<bool>,
    device_clock: DeviceClock,
    stats: BusStats,
}

impl Controller {
    pub fn open(
        geometry: &Geometry,
        option: &TransportOption,
        config: ClientConfig,
    ) -> Result<(Self, Driver), Error> {
        let lost_timeout = option.lost_timeout;
        let config = config.validate()?;
        let num_devices = geometry.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        let bus = Arc::new(UdpBus::open_with_geometry(option, geometry)?);
        let shared = Shared::new(config);
        let driver = Driver::new(Arc::clone(&bus), Arc::clone(&shared));
        let controller = Self {
            geometry: Arc::new(geometry.clone()),
            pool: SlotPool::new(num_devices, config.max_inflight.get()),
            gate: AsyncMutex::new(()),
            bufs: Mutex::new(vec![FrameBuf::new(); num_devices]),
            lost_timeout,
            stopped: AsyncMutex::new(false),
            device_clock: bus.device_clock(),
            stats: bus.stats(),
            shared,
            bus,
        };
        Ok((controller, driver))
    }

    pub async fn initialize(&self) -> Result<(), Error> {
        let _gate = self.gate.lock().await;
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

    #[must_use]
    pub fn config(&self) -> ClientConfig {
        self.shared.pending().config
    }

    pub fn set_config(&self, config: ClientConfig) -> Result<(), Error> {
        let config = config.validate()?;
        {
            let mut pending = self.shared.pending();
            pending.reconfigure(config);
            self.pool.set_limit(config.max_inflight.get());
        }
        tracing::debug!(?config, "client config replaced");
        if !self.bus.wake() {
            tracing::debug!(
                "could not wake the driver; the new ack timeout applies at its next deadline"
            );
        }
        Ok(())
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
        self.enqueue(frame.distribution(), frame.datagrams(), ExecAt::Immediate)
            .await
    }

    pub async fn send_frame_at(
        &self,
        frame: Frame<'_>,
        exec: ExecAt,
    ) -> Result<ResponseFuture, Error> {
        self.enqueue(frame.distribution(), frame.datagrams(), exec)
            .await
    }

    fn closed_error(&self) -> Option<Error> {
        if self.shared.is_stopping() {
            return Some(
                self.shared
                    .pending()
                    .closed_error()
                    .unwrap_or(Error::Closed),
            );
        }
        self.shared.pending().closed_error()
    }

    async fn enqueue(
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
        let _gate = self.gate.lock().await;
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let slot = self.pool.acquire().await;
        if self.shared.pending().need_reset {
            self.reset().await?;
        }
        let exec_at = exec_at::schedule(exec, self.lost_timeout, || self.device_clock.now())?;
        self.transmit(slot, dist, datagrams, exec_at)
    }

    fn transmit(
        &self,
        mut slot: Slot,
        dist: Distribution,
        datagrams: &[Datagram],
        exec_at: u32,
    ) -> Result<ResponseFuture, Error> {
        slot.reset();
        let seq = {
            let mut pending = self.shared.pending();
            let seq = pending.next_seq;
            pending.next_seq = seq.next();
            seq
        };
        tracing::trace!(seq = seq.get(), cmd = ?datagrams[0].cmd, ?dist, "sending frame");
        let (tx, rx) = completion::channel();
        let msg_id = self.bus.reserve_msg_id();
        self.shared.pending().push(Inflight {
            msg_id,
            seq: Some(seq),
            expected: None,
            replied: 0,
            sent_at: Instant::now(),
            slot: Some(slot),
            tx,
        });
        let sent = {
            let mut bufs = self.bufs.lock().unwrap_or_else(PoisonError::into_inner);
            match dist {
                Distribution::Broadcast => {
                    bufs[0].stage(seq, datagrams[0].cmd, exec_at, datagrams[0].payload());
                    self.bus.broadcast(msg_id, bufs[0].as_ref())
                }
                Distribution::PerDevice => {
                    for (buf, datagram) in bufs.iter_mut().zip(datagrams) {
                        buf.stage(seq, datagram.cmd, exec_at, datagram.payload());
                    }
                    self.bus.send(msg_id, &bufs)
                }
            }
        };
        if self.settle(msg_id, sent)? {
            self.stats.record_frame();
        }
        Ok(rx)
    }

    fn settle(&self, msg_id: u16, sent: Result<Sent, UdpError>) -> Result<bool, Error> {
        let (settlement, driver_waits_too_long, ack_timeout) = {
            let mut pending = self.shared.pending();
            let ack_timeout = pending.config.ack_timeout;
            let settlement = pending.settle(msg_id, sent_devices(sent));
            let driver_waits_too_long = matches!(settlement, Settlement::Reached(_))
                && pending.ack_deadline_precedes_driver_wake(msg_id, ack_timeout);
            (settlement, driver_waits_too_long, ack_timeout)
        };
        match settlement {
            Settlement::Failed(entry, e) => {
                drop(entry);
                return Err(e);
            }
            Settlement::Unreached(entry) => {
                if let Some(entry) = entry {
                    entry.tx.send(Err(Error::Timeout {
                        timeout: ack_timeout,
                    }));
                }
                return Ok(false);
            }
            Settlement::Reached(answered) => {
                if let Some(entry) = answered {
                    entry.complete(&self.stats);
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

    pub fn heartbeat(&self) -> Result<(), Error> {
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        if let Err(e) = self.bus.heartbeat() {
            return Err(match e {
                UdpError::Closed => self.closed_error().unwrap_or(Error::Closed),
                e => e.into(),
            });
        }
        let driver_waits_too_long = self.bus.lost_deadline().is_some_and(|deadline| {
            self.shared
                .pending()
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
        let _gate = self.gate.lock().await;
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let (tx, rx) = oneshot::channel();
        self.shared.pending().await_recovery(tx)?;
        if let Err(e) = self.bus.probe() {
            self.shared.pending().recovery = None;
            return Err(e.into());
        }
        if !self.bus.wake() {
            tracing::debug!("could not wake the driver; the recovery settles at its next deadline");
        }
        let recovered = rx.await.unwrap_or(Err(Error::Closed))?;
        tracing::debug!(recovered, "recovery settled");
        if recovered != 0 {
            self.reset().await?;
        }
        Ok(DeviceMask::from_bits(recovered))
    }

    async fn reset(&self) -> Result<(), Error> {
        self.bus.admit_recovered();
        let stale = self.shared.pending().take_all();
        if !stale.is_empty() {
            tracing::debug!(
                stale = stale.len(),
                "failing the frames left in flight before the reset"
            );
            let timeout = self.config().ack_timeout;
            for entry in stale {
                entry.tx.send(Err(Error::Timeout { timeout }));
            }
        }
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        tracing::debug!("sending reset");
        let (tx, rx) = completion::channel();
        let msg_id = self.bus.reserve_msg_id();
        self.shared.pending().push(Inflight {
            msg_id,
            seq: None,
            expected: None,
            replied: 0,
            sent_at: Instant::now(),
            slot: None,
            tx,
        });
        let sent = {
            let mut bufs = self.bufs.lock().unwrap_or_else(PoisonError::into_inner);
            bufs[0].stage(Seq::ZERO, Cmd::Reset, 0, &[]);
            self.bus.broadcast(msg_id, bufs[0].as_ref())
        };
        let reached = self.settle(msg_id, sent)?;
        self.stats.record_reset();
        if !reached {
            return Err(Error::Timeout {
                timeout: self.config().ack_timeout,
            });
        }
        if let Err(e) = rx.await {
            if matches!(e, Error::Timeout { .. }) {
                tracing::warn!("the devices did not acknowledge the reset");
            }
            return Err(e);
        }
        let mut pending = self.shared.pending();
        pending.next_seq = Seq::ZERO;
        pending.need_reset = false;
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
            .enqueue(
                Distribution::Broadcast,
                &[Datagram::no_payload(cmd)],
                ExecAt::Immediate,
            )
            .await?
            .await?;
        response.check()?;
        response.decode(parse)
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
        let mut stopped = self.stopped.lock().await;
        if !*stopped {
            self.silent_stop().await?;
            *stopped = true;
        }
        self.shutdown();
        Ok(())
    }

    pub(crate) fn shutdown(&self) {
        self.shared.request_stop();
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
