mod rt;
#[cfg(test)]
mod tests;

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use autd3_rs_core::CoreId;
use autd3_rs_core::RtPriority;
use autd3_rs_core::RtSchedulePolicy;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::link::{DcClock, IntoLink, Link};
use autd3_rs_core::value::{DcSysTime, Emission};
use std::sync::mpsc;

use autd3_rs_core::rt::{Semaphore, SemaphorePermit, oneshot};

use crate::legacy::datagram::{LegacyDatagramBuilder, LegacyFrame, LegacyFrames};
use crate::legacy::error::{LegacyError, PayloadError};
use crate::legacy::op;
use crate::legacy::wire::{FirmwareVersion, FpgaState, InfoType, Version};

use rt::CmdMessage;

pub const MAX_DEVICES: usize = 128;

const DEFAULT_TIMEOUT_CYCLES: NonZeroU32 = NonZeroU32::new(2000).unwrap();

const SHUTDOWN_POLL: Duration = Duration::from_micros(100);

#[derive(Clone, Copy, Debug)]
pub struct LegacyClientConfig {
    pub timeout_cycles: NonZeroU32,
    pub rt_priority: Option<RtPriority>,
    pub rt_policy: RtSchedulePolicy,
    pub rt_affinity: Option<CoreId>,
}

impl Default for LegacyClientConfig {
    fn default() -> Self {
        Self {
            timeout_cycles: DEFAULT_TIMEOUT_CYCLES,
            rt_priority: autd3_rs_core::default_rt_priority(),
            rt_policy: RtSchedulePolicy::default(),
            rt_affinity: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyResponse {
    data: Vec<u8>,
}

impl LegacyResponse {
    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

pub struct LegacyClient {
    cmd_tx: mpsc::Sender<CmdMessage>,
    send_lock: Semaphore,
    geometry: Arc<Geometry>,
    firmware_version: Vec<FirmwareVersion>,
    reads_fpga_state: AtomicBool,
    join: std::sync::Mutex<Option<JoinHandle<()>>>,
    done: std::sync::Mutex<Option<oneshot::Receiver<()>>>,
    closed: Arc<AtomicBool>,
    shutting_down: AtomicBool,
    dc_clock: Option<DcClock>,
}

impl LegacyClient {
    pub async fn open<T: IntoLink>(
        geometry: &Geometry,
        link: T,
        config: LegacyClientConfig,
    ) -> Result<Self, LegacyError> {
        Self::open_impl(geometry, link, config)
            .await
            .map(|(client, _checker)| client)
    }

    pub async fn open_with_checker<T: IntoLink>(
        geometry: &Geometry,
        link: T,
        config: LegacyClientConfig,
    ) -> Result<(Self, <T::Link as Link>::Checker), LegacyError> {
        Self::open_impl(geometry, link, config).await
    }

    async fn open_impl<T: IntoLink>(
        geometry: &Geometry,
        link: T,
        config: LegacyClientConfig,
    ) -> Result<(Self, <T::Link as Link>::Checker), LegacyError> {
        let link = link.into_link(geometry)?;
        let num_devices = link.num_devices();
        if num_devices == 0 {
            return Err(LegacyError::NoDevices);
        }
        if num_devices > MAX_DEVICES {
            return Err(PayloadError::DeviceCountOutOfRange {
                got: num_devices,
                max: MAX_DEVICES,
            }
            .into());
        }
        if geometry.num_devices() != num_devices {
            return Err(LegacyError::DeviceCountMismatch {
                geometry: geometry.num_devices(),
                link: num_devices,
            });
        }

        let checker = link.state_checker();
        let dc_clock = link.dc_clock();

        let (cmd_tx, cmd_rx) = mpsc::channel::<CmdMessage>();
        let (handshake_tx, handshake_rx) = oneshot::channel();
        let (done_tx, done_rx) = oneshot::channel::<()>();
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_rt = Arc::clone(&closed);
        let join = std::thread::Builder::new()
            .name("autd3-rs-legacy-rt".to_owned())
            .spawn(move || {
                rt::run_rt_thread(link, cmd_rx, config, closed_for_rt, handshake_tx, done_tx);
            })
            .map_err(|e| LegacyError::Link(format!("failed to spawn RT thread: {e}")))?;

        let mut client = Self {
            cmd_tx,
            send_lock: Semaphore::new(1),
            geometry: Arc::new(geometry.clone()),
            firmware_version: Vec::new(),
            reads_fpga_state: AtomicBool::new(false),
            join: std::sync::Mutex::new(Some(join)),
            done: std::sync::Mutex::new(Some(done_rx)),
            closed,
            shutting_down: AtomicBool::new(false),
            dc_clock,
        };

        match handshake_rx.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = client.close().await;
                return Err(e);
            }
            Err(_) => {
                let _ = client.close().await;
                return Err(LegacyError::RtClosed);
            }
        }

        match client.initialize().await {
            Ok(versions) => {
                client.firmware_version = versions;
                tracing::info!(num_devices, "legacy client opened");
                Ok((client, checker))
            }
            Err(e) => {
                let _ = client.close().await;
                Err(e)
            }
        }
    }

    async fn initialize(&self) -> Result<Vec<FirmwareVersion>, LegacyError> {
        let frames = {
            let mut builder = self.datagram_builder();
            builder.push_op(op::Clear::new()).push_op(op::Sync::new());
            builder.build()?
        };
        if let Err(e) = self.send_all(&frames).await {
            return Err(self.explain_initialize_failure(e).await);
        }

        let versions = self.read_firmware_version().await?;
        if let Some(e) = unsupported_firmware(&versions) {
            return Err(e);
        }
        Ok(versions)
    }

    async fn explain_initialize_failure(&self, error: LegacyError) -> LegacyError {
        let Ok(versions) = self.read_firmware_version().await else {
            return error;
        };
        unsupported_firmware(&versions).unwrap_or(error)
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
    pub fn firmware_version(&self) -> &[FirmwareVersion] {
        &self.firmware_version
    }

    #[must_use]
    pub fn dc_offset_ns(&self) -> i64 {
        self.dc_clock
            .as_ref()
            .and_then(DcClock::offset_ns)
            .unwrap_or(0)
    }

    pub fn bus_time_now(&self) -> Result<DcSysTime, LegacyError> {
        Ok(DcSysTime::now()?.with_dc_offset(self.dc_offset_ns()))
    }

    #[must_use]
    pub fn datagram_builder<'a>(&self) -> LegacyDatagramBuilder<'a> {
        self.dc_clock.clone().map_or_else(
            || LegacyDatagramBuilder::new(Arc::clone(&self.geometry)),
            |clock| LegacyDatagramBuilder::with_dc_clock(Arc::clone(&self.geometry), clock),
        )
    }

    pub async fn send(&self, frame: LegacyFrame) -> Result<LegacyResponse, LegacyError> {
        let _guard = self.acquire().await?;
        self.dispatch(frame).await
    }

    pub async fn send_checked(&self, frame: LegacyFrame) -> Result<(), LegacyError> {
        self.send(frame).await.map(|_| ())
    }

    async fn acquire(&self) -> Result<SemaphorePermit<'_>, LegacyError> {
        let guard = self.send_lock.acquire().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err(LegacyError::Closed);
        }
        Ok(guard)
    }

    async fn dispatch(&self, frame: LegacyFrame) -> Result<LegacyResponse, LegacyError> {
        if frame.num_devices() != self.num_devices() {
            return Err(PayloadError::FrameDeviceCountMismatch {
                expected: self.num_devices(),
                got: frame.num_devices(),
            }
            .into());
        }
        let (reply, response) = oneshot::channel();
        self.cmd_tx
            .send(CmdMessage {
                round: frame,
                reply,
            })
            .map_err(|_| LegacyError::RtClosed)?;
        let data = response.await.map_err(|_| LegacyError::RtClosed)??;
        Ok(LegacyResponse { data })
    }

    async fn dispatch_all(&self, frames: &LegacyFrames) -> Result<LegacyResponse, LegacyError> {
        let mut last = LegacyResponse { data: Vec::new() };
        for frame in frames {
            last = self.dispatch(frame).await?;
        }
        Ok(last)
    }

    async fn send_all(&self, frames: &LegacyFrames) -> Result<LegacyResponse, LegacyError> {
        let _guard = self.acquire().await?;
        self.dispatch_all(frames).await
    }

    fn dispatch_blocking(&self, frame: LegacyFrame) -> Result<(), LegacyError> {
        let (reply, mut response) = oneshot::channel();
        self.cmd_tx
            .send(CmdMessage {
                round: frame,
                reply,
            })
            .map_err(|_| LegacyError::RtClosed)?;
        loop {
            match response.try_recv() {
                Some(Ok(result)) => return result.map(|_| ()),
                Some(Err(oneshot::Canceled)) => return Err(LegacyError::RtClosed),
                None => std::thread::sleep(SHUTDOWN_POLL),
            }
        }
    }

    fn dispatch_all_blocking(&self, frames: &LegacyFrames) -> Result<(), LegacyError> {
        for frame in frames {
            self.dispatch_blocking(frame)?;
        }
        Ok(())
    }

    fn build_op<'a, O: op::LegacyOperation + Clone + 'a>(
        &self,
        operation: O,
    ) -> Result<LegacyFrames, LegacyError> {
        let mut builder = self.datagram_builder();
        builder.push_op(operation);
        builder.build()
    }

    async fn send_op<'a, O: op::LegacyOperation + Clone + 'a>(
        &self,
        operation: O,
    ) -> Result<LegacyResponse, LegacyError> {
        let frames = self.build_op(operation)?;
        self.send_all(&frames).await
    }

    async fn dispatch_op<'a, O: op::LegacyOperation + Clone + 'a>(
        &self,
        operation: O,
    ) -> Result<LegacyResponse, LegacyError> {
        let frames = self.build_op(operation)?;
        self.dispatch_all(&frames).await
    }

    async fn fetch_firm_info(&self, ty: InfoType) -> Result<Vec<u8>, LegacyError> {
        Ok(self.dispatch_op(op::FirmInfo::new(ty)).await?.data)
    }

    pub async fn read_firmware_version(&self) -> Result<Vec<FirmwareVersion>, LegacyError> {
        let _guard = self.acquire().await?;
        let result = self.read_firmware_version_impl().await;
        let cleared = self.fetch_firm_info(InfoType::Clear).await;
        let versions = result?;
        cleared?;
        Ok(versions)
    }

    async fn read_firmware_version_impl(&self) -> Result<Vec<FirmwareVersion>, LegacyError> {
        let cpu_major = self.fetch_firm_info(InfoType::CpuMajor).await?;
        let cpu_minor = self.fetch_firm_info(InfoType::CpuMinor).await?;
        let fpga_major = self.fetch_firm_info(InfoType::FpgaMajor).await?;
        let fpga_minor = self.fetch_firm_info(InfoType::FpgaMinor).await?;
        let fpga_functions = self.fetch_firm_info(InfoType::FpgaFunctions).await?;
        Ok((0..self.num_devices())
            .map(|idx| FirmwareVersion {
                idx,
                cpu: Version {
                    major: cpu_major[idx],
                    minor: cpu_minor[idx],
                },
                fpga: Version {
                    major: fpga_major[idx],
                    minor: fpga_minor[idx],
                },
                function_bits: fpga_functions[idx],
            })
            .collect())
    }

    pub async fn read_fpga_state(&self) -> Result<Vec<FpgaState>, LegacyError> {
        let _guard = self.acquire().await?;
        if !self.reads_fpga_state.load(Ordering::Acquire) {
            self.enable_fpga_state_reads().await?;
        }
        let states = self.fetch_fpga_state().await?;
        if first_invalid(&states).is_none() {
            return Ok(states);
        }
        tracing::debug!(
            "a device reported an fpga state with the valid bit clear; re-enabling the reads"
        );
        self.enable_fpga_state_reads().await?;
        let states = self.fetch_fpga_state().await?;
        match first_invalid(&states) {
            Some((device, state)) => Err(LegacyError::FpgaStateInvalid {
                device,
                state: state.0,
            }),
            None => Ok(states),
        }
    }

    async fn enable_fpga_state_reads(&self) -> Result<(), LegacyError> {
        self.dispatch_op(op::ReadsFpgaState::new(true)).await?;
        self.reads_fpga_state.store(true, Ordering::Release);
        Ok(())
    }

    async fn fetch_fpga_state(&self) -> Result<Vec<FpgaState>, LegacyError> {
        Ok(self
            .dispatch_op(op::Nop::new())
            .await?
            .data
            .into_iter()
            .map(FpgaState)
            .collect())
    }

    pub async fn stop(&self) -> Result<(), LegacyError> {
        let null = self
            .geometry
            .iter()
            .map(|d| vec![Emission::NULL; d.num_transducers()])
            .collect::<Vec<_>>();
        self.send_op(op::Gain::new(&null)).await.map(|_| ())
    }

    pub async fn close(&self) -> Result<(), LegacyError> {
        tracing::debug!("closing legacy client");
        let shutdown = {
            let _guard = self.send_lock.acquire().await;
            if self.shutting_down.swap(true, Ordering::AcqRel) {
                Ok(())
            } else {
                self.shutdown_sequence().await
            }
        };
        self.closed.store(true, Ordering::Release);
        let done = self
            .done
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        let joined = match done {
            Some(done) => rt_outcome(done.await),
            None => Ok(()),
        };
        shutdown.and(joined)
    }

    fn shutdown_frames(&self) -> [Result<LegacyFrames, LegacyError>; 3] {
        let null = self
            .geometry
            .iter()
            .map(|d| vec![Emission::NULL; d.num_transducers()])
            .collect::<Vec<_>>();
        [
            self.build_op(op::Silencer::new(op::SilencerConfig::default_non_strict())),
            self.build_op(op::Gain::new(&null)),
            self.build_op(op::Clear::new()),
        ]
    }

    async fn shutdown_sequence(&self) -> Result<(), LegacyError> {
        let mut first_error = Ok(());
        for frames in self.shutdown_frames() {
            let step = match frames {
                Ok(frames) => self.dispatch_all(&frames).await.map(|_| ()),
                Err(e) => Err(e),
            };
            if first_error.is_ok() {
                first_error = step;
            }
        }
        first_error
    }

    fn shutdown_sequence_blocking(&self) -> Result<(), LegacyError> {
        let mut first_error = Ok(());
        for frames in self.shutdown_frames() {
            let step = match frames {
                Ok(frames) => self.dispatch_all_blocking(&frames),
                Err(e) => Err(e),
            };
            if first_error.is_ok() {
                first_error = step;
            }
        }
        first_error
    }
}

impl core::fmt::Debug for LegacyClient {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LegacyClient")
            .field("cmd_tx", &self.cmd_tx)
            .field("send_lock", &self.send_lock)
            .field("geometry", &self.geometry)
            .field("firmware_version", &self.firmware_version)
            .field("reads_fpga_state", &self.reads_fpga_state)
            .field("join", &self.join)
            .field("done", &self.done)
            .field("closed", &self.closed)
            .field("shutting_down", &self.shutting_down)
            .field("dc_clock", &self.dc_clock)
            .finish()
    }
}

impl Drop for LegacyClient {
    fn drop(&mut self) {
        if !self.shutting_down.swap(true, Ordering::AcqRel)
            && let Err(e) = self.shutdown_sequence_blocking()
        {
            tracing::warn!("legacy client failed to mute on drop: {e}");
        }
        self.closed.store(true, Ordering::Release);
        let join = self
            .join
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(join) = join {
            let _ = join.join();
        }
    }
}

fn unsupported_firmware(versions: &[FirmwareVersion]) -> Option<LegacyError> {
    versions
        .iter()
        .find(|version| !version.is_supported())
        .map(|version| LegacyError::UnsupportedFirmware {
            device: version.idx,
            version: version.cpu.to_string(),
        })
}

fn first_invalid(states: &[FpgaState]) -> Option<(usize, FpgaState)> {
    states
        .iter()
        .enumerate()
        .find(|(_, state)| !state.is_valid())
        .map(|(device, state)| (device, *state))
}

fn rt_outcome(done: Result<(), oneshot::Canceled>) -> Result<(), LegacyError> {
    done.map_err(|_| LegacyError::Link("RT thread panicked".to_owned()))
}
