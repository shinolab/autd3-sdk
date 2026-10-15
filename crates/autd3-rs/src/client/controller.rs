use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_lock::Mutex as AsyncMutex;
use autd3_cpu_wire::payload::FirmwareInfo;
use zerocopy::FromBytes;

use crate::commands::operation::{Clear, Distribution, FixedCompletionTime, Synchronize};
use crate::commands::{Command, Pattern, SetSilencer};
use crate::datagram::{Datagram, Frame, Frames};
use crate::error::{Error, NetworkCause, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::link::Link;
use crate::protocol::{Cmd, Seq};
use crate::telemetry::Telemetry;
use crate::udp::frame_buf::{self, FrameBuf};
use crate::udp::{Sent, StateChecker, UdpError};
use crate::value::{Intensity, SysTime};
use autd3_rs_core::{BusStats, DeviceClock, DeviceState};

use super::completion::{self, ResponseFuture, StreamFuture};
use super::config::{ClientConfig, MAX_DEVICES};
use super::driver::{Driver, Inflight, Settlement, Shared, sent_devices};
use super::pool::{Slot, SlotPool};
use super::transport::{Dispatch, Transport};

#[derive(Clone, Copy, Debug)]
enum Restoration {
    Synchronize(u128),
    ReleaseFailsafe(u128),
}

pub struct Controller {
    geometry: Arc<Geometry>,
    transport: Arc<Transport>,
    shared: Arc<Shared>,
    pool: Arc<SlotPool>,
    gate: AsyncMutex<()>,
    bufs: Mutex<Vec<FrameBuf>>,
    config: ClientConfig,
    stopped: AsyncMutex<bool>,
    device_clock: DeviceClock,
    exec_lead: Option<Duration>,
    stats: BusStats,
}

impl Controller {
    pub fn open(
        geometry: &Geometry,
        link: impl Into<Link>,
        config: ClientConfig,
    ) -> Result<(Self, Driver), Error> {
        let config = config.validate()?;
        let num_devices = geometry.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        let link = link.into();
        if let (Some(exec_lead), Link::Udp(option)) = (config.exec_lead, &link)
            && exec_lead >= option.lost_timeout
        {
            return Err(PayloadError::ExecLeadNotBelowLostTimeout {
                exec_lead,
                lost_timeout: option.lost_timeout,
            }
            .into());
        }
        let (transport, port) = Transport::open(&link, geometry)?;
        let shared = Shared::new();
        let driver = Driver::new(
            Arc::clone(&transport),
            port,
            Arc::clone(&shared),
            config.ack_timeout,
            config.resynchronize_on_recovery.is_some(),
            config.release_failsafe_on_recovery.is_some(),
        );
        let controller = Self {
            geometry: Arc::new(geometry.clone()),
            pool: SlotPool::new(num_devices, config.max_inflight.get()),
            gate: AsyncMutex::new(()),
            bufs: Mutex::new(vec![FrameBuf::new(); num_devices]),
            config,
            stopped: AsyncMutex::new(false),
            device_clock: transport.device_clock(),
            exec_lead: config.exec_lead,
            stats: transport.stats(),
            shared,
            transport,
        };
        Ok((controller, driver))
    }

    pub async fn initialize(&self) -> Result<(), Error> {
        {
            let _gate = self.gate.lock().await;
            self.reset().await?;
        }
        tracing::debug!("handshake complete");
        self.check_firmware_version(self.config.require_supported_firmware)
            .await?;
        self.send(Clear).await?;
        self.send(Synchronize).await?;
        tracing::info!(num_devices = self.num_devices(), "controller initialized");
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
        self.transport.state_checker()
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

    pub fn send_streaming<'a, C: Command<'a>>(
        &self,
        cmd: C,
    ) -> impl Future<Output = Result<StreamFuture, Error>> + use<'_, C> {
        let frames = Frames::encode(&self.geometry, cmd);
        async move {
            let frames = frames?;
            let mut stream = StreamFuture::default();
            for frame in &frames {
                if stream.in_flight() >= self.config.max_inflight.get() {
                    stream.settle_oldest().await;
                }
                if stream.has_failed() {
                    break;
                }
                match self.send_frame(frame).await {
                    Ok(response) => stream.push(response),
                    Err(e) => {
                        stream.fail(e);
                        break;
                    }
                }
            }
            Ok(stream)
        }
    }

    pub async fn send_frame(&self, frame: Frame<'_>) -> Result<ResponseFuture, Error> {
        self.enqueue(frame.distribution(), frame.datagrams()).await
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
        let _gate = self.gate.lock().await;
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let slot = loop {
            let slot = self.pool.acquire().await;
            if self.shared.pending().need_reset {
                self.reset().await?;
            }
            let Some(restoration) = self.next_restoration()? else {
                break slot;
            };
            self.restore(slot, restoration).await?;
        };
        let exec_at = self.exec_at()?;
        self.transmit(slot, dist, datagrams, exec_at)
    }

    fn exec_at(&self) -> Result<u32, Error> {
        frame_buf::schedule(self.exec_lead, || self.device_clock.now())
    }

    fn next_restoration(&self) -> Result<Option<Restoration>, Error> {
        let (resync, release_failsafe) = {
            let pending = self.shared.pending();
            if pending.need_reset {
                return Ok(None);
            }
            (pending.resync.devices, pending.release_failsafe.devices)
        };
        if resync | release_failsafe == 0 {
            return Ok(None);
        }
        let status = self.transport.state_checker().check()?;
        let ready = status
            .devices()
            .iter()
            .enumerate()
            .filter(|&(_, &state)| state == DeviceState::Ready)
            .fold(0u128, |ready, (device, _)| ready | 1 << device);
        let synchronize = resync & ready;
        if synchronize != 0 {
            return Ok(Some(Restoration::Synchronize(synchronize)));
        }
        let release = release_failsafe & ready;
        Ok((release != 0).then_some(Restoration::ReleaseFailsafe(release)))
    }

    async fn restore(&self, slot: Slot, restoration: Restoration) -> Result<(), Error> {
        tracing::debug!(?restoration, "restoring the recovered devices");
        let (cmd, recovered) = match restoration {
            Restoration::Synchronize(recovered) => (Cmd::Synchronize, recovered),
            Restoration::ReleaseFailsafe(recovered) => (Cmd::ReleaseFailsafe, recovered),
        };
        let datagrams: Vec<Datagram> = (0..self.num_devices())
            .map(|device| {
                Datagram::no_payload(if recovered & (1 << device) == 0 {
                    Cmd::Nop
                } else {
                    cmd
                })
            })
            .collect();
        let outcome = self.attempt(slot, &datagrams).await;
        let mut pending = self.shared.pending();
        let (restoring, attempts) = match restoration {
            Restoration::Synchronize(_) => {
                (&mut pending.resync, self.config.resynchronize_on_recovery)
            }
            Restoration::ReleaseFailsafe(_) => (
                &mut pending.release_failsafe,
                self.config.release_failsafe_on_recovery,
            ),
        };
        match outcome {
            Ok(replied) => {
                restoring.restored(recovered & replied);
                Ok(())
            }
            Err(e) => {
                if restoring.failed(recovered, attempts.unwrap_or(NonZeroU32::MIN)) {
                    tracing::warn!(
                        ?restoration,
                        error = %e,
                        "giving up restoring the recovered devices"
                    );
                }
                Err(e)
            }
        }
    }

    async fn attempt(&self, slot: Slot, datagrams: &[Datagram]) -> Result<u128, Error> {
        let response = self
            .transmit(slot, Distribution::PerDevice, datagrams, 0)?
            .await?;
        response.check()?;
        Ok(response.replied())
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
        let msg_id = self.transport.reserve_msg_id();
        self.shared.pending().push(Inflight {
            msg_id,
            seq: Some(seq),
            expected: None,
            replied: 0,
            sent_at: Instant::now(),
            slot: Some(slot),
            tx,
        });
        let dispatch = {
            let mut bufs = self.bufs.lock().unwrap_or_else(PoisonError::into_inner);
            match dist {
                Distribution::Broadcast => {
                    bufs[0].stage(seq, datagrams[0].cmd, exec_at, datagrams[0].payload());
                    self.transport.send(msg_id, true, &bufs)
                }
                Distribution::PerDevice => {
                    for (buf, datagram) in bufs.iter_mut().zip(datagrams) {
                        buf.stage(seq, datagram.cmd, exec_at, datagram.payload());
                    }
                    self.transport.send(msg_id, false, &bufs)
                }
            }
        };
        if self.dispatched(msg_id, dispatch)? {
            self.stats.record_frame();
        }
        Ok(rx)
    }

    fn dispatched(&self, msg_id: u16, dispatch: Dispatch) -> Result<bool, Error> {
        match dispatch {
            Dispatch::Settled(sent) => self.settle(msg_id, sent),
            Dispatch::Deferred => Ok(false),
            Dispatch::Failed(e) => {
                drop(self.shared.pending().withdraw(msg_id));
                Err(Error::Network(NetworkCause::new(e)))
            }
        }
    }

    fn settle(&self, msg_id: u16, sent: Result<Sent, UdpError>) -> Result<bool, Error> {
        let (settlement, driver_waits_too_long) = {
            let mut pending = self.shared.pending();
            let settlement = pending.settle(msg_id, sent_devices(sent));
            let driver_waits_too_long = matches!(settlement, Settlement::Reached(_))
                && pending.ack_deadline_precedes_driver_wake(msg_id, self.config.ack_timeout);
            (settlement, driver_waits_too_long)
        };
        match settlement {
            Settlement::Failed(_, e) => return Err(e),
            Settlement::Unreached(entry) => {
                if let Some(entry) = entry {
                    entry.tx.send(Err(Error::Timeout {
                        timeout: self.config.ack_timeout,
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
        if driver_waits_too_long && !self.transport.wake() {
            tracing::debug!(
                msg_id,
                "could not wake the driver; the ack timeout is noticed at its next deadline"
            );
        }
        Ok(true)
    }

    async fn reset(&self) -> Result<(), Error> {
        let recoveries = self.shared.pending().recoveries;
        self.transport.admit_recovered();
        let stale = self.shared.pending().take_all();
        if !stale.is_empty() {
            tracing::debug!(
                stale = stale.len(),
                "failing the frames left in flight before the reset"
            );
            let timeout = self.config.ack_timeout;
            for entry in stale {
                entry.tx.send(Err(Error::Timeout { timeout }));
            }
        }
        for round in 0..self.config.max_resync_rounds.get() {
            if let Some(e) = self.closed_error() {
                return Err(e);
            }
            tracing::debug!(round, "sending reset");
            let (tx, rx) = completion::channel();
            let msg_id = self.transport.reserve_msg_id();
            self.shared.pending().push(Inflight {
                msg_id,
                seq: None,
                expected: None,
                replied: 0,
                sent_at: Instant::now(),
                slot: None,
                tx,
            });
            let dispatch = {
                let mut bufs = self.bufs.lock().unwrap_or_else(PoisonError::into_inner);
                bufs[0].stage(Seq::ZERO, Cmd::Reset, 0, &[]);
                self.transport.send(msg_id, true, &bufs)
            };
            let deferred = matches!(dispatch, Dispatch::Deferred);
            let reached = self.dispatched(msg_id, dispatch)?;
            self.stats.record_reset();
            if !(reached || deferred) {
                return Err(Error::Timeout {
                    timeout: self.config.ack_timeout,
                });
            }
            match rx.await {
                Ok(_) => {
                    let mut pending = self.shared.pending();
                    pending.next_seq = Seq::ZERO;
                    pending.need_reset = pending.recoveries != recoveries;
                    tracing::debug!("reset confirmed by every device");
                    return Ok(());
                }
                Err(Error::Timeout { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        tracing::warn!(
            rounds = self.config.max_resync_rounds.get(),
            "the devices did not acknowledge the reset"
        );
        Err(Error::Timeout {
            timeout: self.config.ack_timeout,
        })
    }

    pub async fn silent_stop(&self) -> Result<(), Error> {
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
            .enqueue(Distribution::Broadcast, &[Datagram::no_payload(cmd)])
            .await?
            .await?;
        response.check()?;
        response.decode(parse)
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, Error> {
        let versions = self
            .read_values(Cmd::ReadFirmwareInfo, |value| {
                FirmwareInfo::read_from_prefix(value)
                    .ok()
                    .map(|(info, _)| FirmwareVersion::from_info(info))
            })
            .await?;

        let (major, minor) = FirmwareVersion::SUPPORTED_SERIES;
        for (device, version) in versions.iter().enumerate() {
            if !version.is_supported() {
                tracing::warn!(
                    device,
                    "firmware {version} is outside the series supported by this SDK ({major}.{minor}.x); correct operation is not guaranteed"
                );
            }
        }

        Ok(versions)
    }

    async fn check_firmware_version(&self, require_supported: bool) -> Result<(), Error> {
        let versions = match self.read_firmware_version().await {
            Ok(versions) => versions,
            Err(e) if require_supported => return Err(e),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "could not read the firmware version, so the series check was skipped"
                );
                return Ok(());
            }
        };
        if !require_supported {
            return Ok(());
        }
        match versions
            .into_iter()
            .enumerate()
            .find(|(_, version)| !version.is_supported())
        {
            Some((device, version)) => Err(Error::UnsupportedFirmware { device, version }),
            None => Ok(()),
        }
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
        if !self.transport.wake() {
            tracing::warn!("could not wake the driver; it stops at its next deadline");
        }
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shutdown();
    }
}
