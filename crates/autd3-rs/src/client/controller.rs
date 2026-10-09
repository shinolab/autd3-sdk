use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use async_lock::Mutex as AsyncMutex;
use autd3_cpu_wire::payload::FirmwareInfo;
use zerocopy::FromBytes;

use crate::commands::operation::{Clear, Distribution, FixedCompletionTime, Synchronize};
use crate::commands::{Command, Pattern, SetSilencer};
use crate::datagram::{Datagram, Frame, Frames};
use crate::error::{Error, PayloadError};
use crate::firmware_version::FirmwareVersion;
use crate::fpga_state::FpgaState;
use crate::geometry::Geometry;
use crate::protocol::{Cmd, Seq};
use crate::telemetry::TelemetryCounters;
use crate::udp::frame_buf::FrameBuf;
use crate::udp::{Sent, StateChecker, TransportOption, UdpBus, UdpError};
use crate::value::{Intensity, SysTime};
use autd3_rs_core::{BusStats, DeviceClock};

use super::completion::{self, ResponseFuture, StreamFuture};
use super::config::{ClientConfig, MAX_DEVICES};
use super::driver::{Driver, Inflight, Shared};
use super::pool::SlotPool;

pub struct Controller {
    geometry: Arc<Geometry>,
    bus: Arc<UdpBus>,
    shared: Arc<Shared>,
    pool: Arc<SlotPool>,
    gate: AsyncMutex<()>,
    bufs: Mutex<Vec<FrameBuf>>,
    config: ClientConfig,
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
        let config = config.validate()?;
        let num_devices = geometry.num_devices();
        if num_devices == 0 || num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        let bus = Arc::new(UdpBus::open(option, num_devices)?);
        let shared = Shared::new();
        let driver = Driver::new(Arc::clone(&bus), Arc::clone(&shared), config.ack_timeout);
        let controller = Self {
            geometry: Arc::new(geometry.clone()),
            pool: SlotPool::new(num_devices, config.max_inflight.get()),
            gate: AsyncMutex::new(()),
            bufs: Mutex::new(vec![FrameBuf::new(); num_devices]),
            config,
            stopped: AsyncMutex::new(false),
            device_clock: bus.device_clock(),
            stats: bus.stats(),
            shared,
            bus,
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
        let mut slot = self.pool.acquire().await;
        slot.reset();
        if self.shared.pending().need_reset {
            self.reset().await?;
        }
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
                    bufs[0].stage(seq, datagrams[0].cmd, datagrams[0].payload());
                    self.bus.send_broadcast_as(msg_id, bufs[0].as_ref())
                }
                Distribution::PerDevice => {
                    for (buf, datagram) in bufs.iter_mut().zip(datagrams) {
                        buf.stage(seq, datagram.cmd, datagram.payload());
                    }
                    self.bus.send_as(msg_id, &bufs)
                }
            }
        };
        if self.settle(msg_id, sent)? {
            self.stats.record_frame();
        }
        Ok(rx)
    }

    fn settle(&self, msg_id: u16, sent: Result<Sent, UdpError>) -> Result<bool, Error> {
        let sent = sent.and_then(|sent| match sent.unsent {
            Some(unsent) => Err(unsent.into()),
            None => Ok(sent.devices),
        });
        let devices = match sent {
            Ok(devices) => devices,
            Err(e) => {
                let mut pending = self.shared.pending();
                if matches!(e, UdpError::Unsent { .. }) {
                    pending.need_reset = true;
                }
                drop(pending.withdraw(msg_id));
                return Err(e.into());
            }
        };
        if devices == 0 {
            if let Some(entry) = self.shared.pending().withdraw(msg_id) {
                entry.tx.send(Err(Error::Timeout {
                    timeout: self.config.ack_timeout,
                }));
            }
            return Ok(false);
        }
        let (answered, driver_waits_too_long) = {
            let mut pending = self.shared.pending();
            let answered = pending.sent(msg_id, devices);
            let driver_waits_too_long =
                pending.ack_deadline_precedes_driver_wake(msg_id, self.config.ack_timeout);
            (answered, driver_waits_too_long)
        };
        if let Some(entry) = answered {
            entry.complete(&self.stats);
        }
        if driver_waits_too_long && !self.bus.wake() {
            tracing::debug!(
                msg_id,
                "could not wake the driver; the ack timeout is noticed at its next deadline"
            );
        }
        Ok(true)
    }

    async fn reset(&self) -> Result<(), Error> {
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
                bufs[0].stage(Seq::ZERO, Cmd::Reset, &[]);
                self.bus.send_broadcast_as(msg_id, bufs[0].as_ref())
            };
            let reached = self.settle(msg_id, sent)?;
            self.stats.record_reset();
            if !reached {
                return Err(Error::Timeout {
                    timeout: self.config.ack_timeout,
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

    pub async fn read_telemetry(&self) -> Result<Vec<TelemetryCounters>, Error> {
        self.read_values(Cmd::ReadTelemetry, TelemetryCounters::parse)
            .await
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

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use autd3_rs_core::DeviceState;
    use autd3_rs_firmware_emulator::udp::UdpEmulator;
    use pollster::block_on;

    use crate::commands::Nop;
    use crate::geometry::Autd3;

    use super::*;

    const REFUSED_BY_THE_HOST: ErrorKind = ErrorKind::PermissionDenied;
    const NEVER_LOST: Duration = Duration::from_secs(60);

    fn open(
        emulator: &UdpEmulator,
        lost_timeout: Duration,
    ) -> (Controller, JoinHandle<Result<(), Error>>) {
        let geometry = Geometry::new(
            (0..emulator.num_devices())
                .map(|_| Autd3::default())
                .collect(),
        );
        let option = TransportOption {
            iface: emulator.interface(),
            reply_timeout: Duration::from_millis(50),
            response_timeout: Duration::from_millis(50),
            enumeration_timeout: Duration::from_secs(1),
            sync_timeout: Duration::from_secs(5),
            lost_timeout,
            ..TransportOption::default()
        };
        let (controller, driver) =
            Controller::open(&geometry, &option, ClientConfig::default()).unwrap();
        let io = std::thread::spawn(move || driver.run());
        block_on(controller.initialize()).unwrap();
        (controller, io)
    }

    fn unsent_device(result: Result<(), Error>) -> Option<usize> {
        let Err(Error::Network(cause)) = result else {
            return None;
        };
        match cause.downcast_ref::<UdpError>() {
            Some(UdpError::Unsent { device, .. }) => Some(*device),
            _ => None,
        }
    }

    #[test]
    fn a_frame_one_unit_could_not_be_sent_fails_and_the_next_send_realigns_the_sequence() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let (controller, io) = open(&emulator, NEVER_LOST);

        controller.bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        assert_eq!(unsent_device(block_on(controller.send(Nop))), Some(1));
        assert_eq!(unsent_device(block_on(controller.send(Nop))), Some(1));

        controller.bus.restore_sends();
        block_on(controller.send(Nop)).unwrap();
        block_on(controller.send(Nop)).unwrap();
        block_on(controller.close()).unwrap();
        io.join().unwrap().unwrap();
    }

    #[test]
    fn a_close_whose_stop_frame_could_not_be_sent_to_a_unit_reports_it() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let (controller, io) = open(&emulator, NEVER_LOST);

        controller.bus.fail_sends_to(0, REFUSED_BY_THE_HOST);
        assert_eq!(unsent_device(block_on(controller.close())), Some(0));
        assert!(!io.is_finished());

        controller.bus.restore_sends();
        block_on(controller.close()).unwrap();
        io.join().unwrap().unwrap();
    }

    #[test]
    fn heartbeats_that_cannot_be_sent_do_not_close_the_client() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let (controller, io) = open(&emulator, NEVER_LOST);

        controller.bus.fail_sends_to(0, REFUSED_BY_THE_HOST);
        controller.bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        std::thread::sleep(Duration::from_millis(20));
        let heartbeats = controller.bus_stats().heartbeats();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(controller.bus_stats().heartbeats(), heartbeats);
        assert!(!io.is_finished());
        assert!(controller.closed_error().is_none());

        controller.bus.restore_sends();
        block_on(controller.send(Nop)).unwrap();
        assert_eq!(
            controller.state_checker().check().unwrap().devices(),
            [DeviceState::Ready; 2]
        );
        block_on(controller.close()).unwrap();
        io.join().unwrap().unwrap();
    }

    #[test]
    fn a_unit_whose_sends_keep_failing_becomes_lost_and_is_skipped_afterwards() {
        let emulator = UdpEmulator::spawn(2).unwrap();
        let (controller, io) = open(&emulator, Duration::from_millis(100));
        let checker = controller.state_checker();

        controller.bus.fail_sends_to(1, REFUSED_BY_THE_HOST);
        let deadline = Instant::now() + Duration::from_secs(5);
        while checker.check().unwrap().devices()[1] != DeviceState::Lost {
            assert!(Instant::now() < deadline, "device 1 never became lost");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(checker.check().unwrap().devices()[0], DeviceState::Ready);

        block_on(controller.send(Nop)).unwrap();
        block_on(controller.close()).unwrap();
        io.join().unwrap().unwrap();
    }
}
