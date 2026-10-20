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
use crate::udp::frame_buf::FrameBuf;
use crate::udp::{Sent, StateChecker, UdpError};
use crate::value::{Intensity, SysTime};
use autd3_rs_core::{BusStats, DeviceClock, DeviceMask};
use futures_channel::oneshot;

use super::completion::{self, ResponseFuture, StreamFuture};
use super::config::{ClientConfig, MAX_DEVICES};
use super::driver::{Driver, Inflight, Settlement, Shared, sent_devices};
use super::exec_at::{self, ExecAt};
use super::pool::{Slot, SlotPool};
use super::transport::{Dispatch, Transport};

pub struct Controller {
    geometry: Arc<Geometry>,
    transport: Arc<Transport>,
    shared: Arc<Shared>,
    pool: Arc<SlotPool>,
    gate: AsyncMutex<()>,
    bufs: Mutex<Vec<FrameBuf>>,
    lost_timeout: Option<Duration>,
    stopped: AsyncMutex<bool>,
    device_clock: DeviceClock,
    stats: BusStats,
}

impl Controller {
    pub fn open(
        geometry: &Geometry,
        link: impl Into<Link>,
        config: ClientConfig,
    ) -> Result<(Self, Driver), Error> {
        let link = link.into();
        let lost_timeout = match &link {
            Link::Udp(option) => Some(option.lost_timeout),
            Link::Server(_) => None,
        };
        let config = config.validate(lost_timeout)?;
        let num_devices = geometry.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        let (transport, port) = Transport::open(&link, geometry)?;
        let shared = Shared::new(config);
        let driver = Driver::new(Arc::clone(&transport), port, Arc::clone(&shared));
        let controller = Self {
            geometry: Arc::new(geometry.clone()),
            pool: SlotPool::new(num_devices, config.max_inflight.get()),
            gate: AsyncMutex::new(()),
            bufs: Mutex::new(vec![FrameBuf::new(); num_devices]),
            lost_timeout,
            stopped: AsyncMutex::new(false),
            device_clock: transport.device_clock(),
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
        self.check_firmware_version(self.config().require_supported_firmware)
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

    #[must_use]
    pub fn config(&self) -> ClientConfig {
        self.shared.pending().config
    }

    pub fn set_config(&self, config: ClientConfig) -> Result<(), Error> {
        let config = config.validate(self.lost_timeout)?;
        {
            let mut pending = self.shared.pending();
            pending.reconfigure(config);
            self.pool.set_limit(config.max_inflight.get());
        }
        tracing::debug!(?config, "client config replaced");
        if !self.transport.wake() {
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

    pub fn send_streaming<'a, C: Command<'a>>(
        &self,
        cmd: C,
    ) -> impl Future<Output = Result<StreamFuture, Error>> + use<'_, C> {
        let frames = Frames::encode(&self.geometry, cmd);
        async move {
            let frames = frames?;
            let mut stream = StreamFuture::default();
            for frame in &frames {
                if stream.in_flight() >= self.config().max_inflight.get() {
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
        self.enqueue(frame.distribution(), frame.datagrams(), None)
            .await
    }

    pub async fn send_frame_at(
        &self,
        frame: Frame<'_>,
        exec: ExecAt,
    ) -> Result<ResponseFuture, Error> {
        self.enqueue(frame.distribution(), frame.datagrams(), Some(exec))
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
        exec: Option<ExecAt>,
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
        let slot = self.pool.acquire().await;
        if self.shared.pending().need_reset {
            self.reset().await?;
        }
        let exec_at = self.exec_at(exec)?;
        self.transmit(slot, dist, datagrams, exec_at)
    }

    fn exec_at(&self, exec: Option<ExecAt>) -> Result<u32, Error> {
        let exec = exec.unwrap_or_else(|| ExecAt::from_exec_lead(self.config().exec_lead));
        exec_at::schedule(exec, self.lost_timeout, || self.device_clock.now())
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
        let (settlement, driver_waits_too_long, ack_timeout) = {
            let mut pending = self.shared.pending();
            let ack_timeout = pending.config.ack_timeout;
            let settlement = pending.settle(msg_id, sent_devices(sent));
            let driver_waits_too_long = matches!(settlement, Settlement::Reached(_))
                && pending.ack_deadline_precedes_driver_wake(msg_id, ack_timeout);
            (settlement, driver_waits_too_long, ack_timeout)
        };
        match settlement {
            Settlement::Failed(_, e) => return Err(e),
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
        if driver_waits_too_long && !self.transport.wake() {
            tracing::debug!(
                msg_id,
                "could not wake the driver; the ack timeout is noticed at its next deadline"
            );
        }
        Ok(true)
    }

    pub async fn recover(&self) -> Result<DeviceMask, Error> {
        let _gate = self.gate.lock().await;
        if let Some(e) = self.closed_error() {
            return Err(e);
        }
        let (tx, rx) = oneshot::channel();
        self.shared.pending().await_recovery(tx)?;
        if let Err(e) = self.transport.probe() {
            self.shared.pending().recovery = None;
            return Err(e);
        }
        let recovered = rx.await.unwrap_or(Err(Error::Closed))?;
        tracing::debug!(recovered, "recovery settled");
        if recovered != 0 {
            self.reset().await?;
        }
        Ok(DeviceMask::from_bits(recovered))
    }

    async fn reset(&self) -> Result<(), Error> {
        self.transport.admit_recovered();
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
        let rounds = self.config().max_resync_rounds.get();
        for round in 0..rounds {
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
                    timeout: self.config().ack_timeout,
                });
            }
            match rx.await {
                Ok(_) => {
                    let mut pending = self.shared.pending();
                    pending.next_seq = Seq::ZERO;
                    pending.need_reset = false;
                    tracing::debug!("reset confirmed by every device");
                    return Ok(());
                }
                Err(Error::Timeout { .. }) => {}
                Err(e) => return Err(e),
            }
        }
        tracing::warn!(rounds, "the devices did not acknowledge the reset");
        Err(Error::Timeout {
            timeout: self.config().ack_timeout,
        })
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
            .enqueue(Distribution::Broadcast, &[Datagram::no_payload(cmd)], None)
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
