use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll, Wake, Waker};
use std::thread::{JoinHandle, Thread};
use std::time::Duration;

use autd3_cpu_wire::describe_device_error;
use autd3_cpu_wire::fpga_params::FunctionBits;
use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{FirmwareInfo, UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::update::RunningImage;
use autd3_rs::commands::Distribution;
use autd3_rs::geometry::{Autd3, Geometry};
use autd3_rs::protocol::{Cmd, FrameHeader, PAYLOAD_BYTES};
use autd3_rs::{
    Client, Datagram, Error, Frame as ClientFrame, NetworkCause, Response, ResponseFuture,
    StateChecker, TransportOption,
};
use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::fpga_image::FpgaFirmwareImage;
use crate::image::CpuFirmwareImage;

pub const DEFAULT_HEARTBEAT: Duration = Duration::from_millis(10);
pub const ACK_TIMEOUT: Duration = Duration::from_secs(120);
pub const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
pub const MIN_CPU_FIRMWARE_VERSION: [u8; 3] = [0, 10, 0];

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("network error: {0}")]
    Network(#[source] Box<dyn core::error::Error + Send + Sync>),
    #[error("not every device acknowledged {cmd:?}; {}", timeout_hint(*cmd))]
    Timeout { cmd: Cmd },
    #[error("device {device} rejected {cmd:?} with firmware error {code:#04x}: {}{}", describe_device_error(*code), device_hint(*cmd, *code))]
    Device { device: usize, cmd: Cmd, code: u8 },
    #[error("device {device} returned a shorter reply to {cmd:?} than the command defines")]
    ShortReply { device: usize, cmd: Cmd },
    #[error(
        "device {device} runs an FPGA image without configuration-flash access; flash `autd3-fpga.mcs` once via JTAG"
    )]
    FpgaUpdateUnsupported { device: usize },
    #[error(
        "device {device} did not reconfigure after activation (or an earlier attempt failed since power-on); the written image boots at the next power cycle"
    )]
    FpgaReconfigFailed { device: usize },
    #[error(
        "device {device} runs CPU firmware {}.{}.{}, but the update needs {}.{}.{} or newer; flash it once via J-Link",
        found[0], found[1], found[2], required[0], required[1], required[2]
    )]
    UnsupportedFirmware {
        device: usize,
        found: [u8; 3],
        required: [u8; 3],
    },
}

fn timeout_hint(cmd: Cmd) -> &'static str {
    match cmd {
        Cmd::UpdateBegin | Cmd::UpdateChunk | Cmd::UpdateCommit => {
            "the device keeps the half-written slot and boots its previous image; rerun the update from the beginning"
        }
        Cmd::FpgaUpdateBegin | Cmd::FpgaUpdateChunk | Cmd::FpgaUpdateCommit => {
            "the device stays locked with its output off until the next power cycle; power-cycle it and rerun the FPGA update"
        }
        Cmd::UpdateActivate => {
            "the image is committed and the devices that received the request are rebooting into it as a trial; wait for the reboot, check which devices run it with `--verify-only`, then run `--confirm-only` (a device that did not reboot keeps its previous image and boots the new one at its next power cycle)"
        }
        Cmd::UpdateConfirm => {
            "the running image stays unconfirmed on the devices that did not answer; check the cable and rerun with `--confirm-only` before the next power cycle"
        }
        Cmd::Reboot => {
            "the devices that received the request are resetting and come back unassigned; rerun `--reboot-only` to reset the rest"
        }
        Cmd::Synchronize
        | Cmd::Clear
        | Cmd::Nop
        | Cmd::SetCpuConfig
        | Cmd::ReleaseFailsafe
        | Cmd::SetFpgaConfig
        | Cmd::WriteFociBuffer
        | Cmd::ConfigPattern
        | Cmd::ActivatePatternBank
        | Cmd::WritePatternPhase
        | Cmd::WritePatternRaw
        | Cmd::WritePatternFused
        | Cmd::WriteModulationBuffer
        | Cmd::ConfigModulation
        | Cmd::ActivateModulationBank
        | Cmd::SetSilencer
        | Cmd::SetPhaseCorrection
        | Cmd::SetOutputMask
        | Cmd::SetPulseWidthTable
        | Cmd::EmulateGpioIn
        | Cmd::SetGpioOut
        | Cmd::ForceFan
        | Cmd::FpgaUpdateActivate
        | Cmd::ReadFpgaState
        | Cmd::ReadTelemetry
        | Cmd::ReadFirmwareInfo
        | Cmd::ReadRunningImage
        | Cmd::WriteFpgaMemory
        | Cmd::ReadFpgaMemory => "check the cable and the host firewall, then rerun",
        _ => unreachable!(),
    }
}

fn device_hint(cmd: Cmd, code: u8) -> &'static str {
    match (cmd, autd3_cpu_wire::Error::from_u8(code)) {
        (Cmd::UpdateActivate, Some(autd3_cpu_wire::Error::FpgaUpdateInProgress)) => {
            " (the CPU image is already committed: it boots as a trial at the next power cycle, run `--confirm-only` after that)"
        }
        (
            Cmd::UpdateBegin | Cmd::FpgaUpdateBegin,
            Some(autd3_cpu_wire::Error::UpdateActivating),
        ) => " (wait for the reboot, then reconnect and rerun)",
        (_, Some(autd3_cpu_wire::Error::UpdateImageInvalid)) => {
            " (nothing was activated; the previous image keeps running, rerun the update)"
        }
        (
            _,
            None
            | Some(
                autd3_cpu_wire::Error::None
                | autd3_cpu_wire::Error::UnknownCmd
                | autd3_cpu_wire::Error::InvalidPayload
                | autd3_cpu_wire::Error::InvalidSilencerSetting
                | autd3_cpu_wire::Error::InvalidTransitionMode
                | autd3_cpu_wire::Error::MissTransitionTime
                | autd3_cpu_wire::Error::FpgaTimeout
                | autd3_cpu_wire::Error::SyncNotReady
                | autd3_cpu_wire::Error::UpdateNotStarted
                | autd3_cpu_wire::Error::UpdateFlash
                | autd3_cpu_wire::Error::UpdateNotCommitted
                | autd3_cpu_wire::Error::UpdateNothingToConfirm
                | autd3_cpu_wire::Error::UpdateUnsupported
                | autd3_cpu_wire::Error::FpgaUpdateInProgress
                | autd3_cpu_wire::Error::UpdateActivating
                | autd3_cpu_wire::Error::SyncMissed
                | autd3_cpu_wire::Error::FailsafeConditionActive
                | autd3_cpu_wire::Error::FpgaUnsupported,
            ),
        ) => "",
        (_, Some(_)) => unreachable!(),
    }
}

fn first_unsupported(infos: &[FirmwareInfo]) -> Option<(usize, [u8; 3])> {
    infos
        .iter()
        .map(|info| info.cpu_version)
        .enumerate()
        .find(|&(_, version)| version < MIN_CPU_FIRMWARE_VERSION)
}

fn ensure_cpu_supported(infos: &[FirmwareInfo]) -> Result<(), DriverError> {
    match first_unsupported(infos) {
        None => Ok(()),
        Some((device, found)) => Err(DriverError::UnsupportedFirmware {
            device,
            found,
            required: MIN_CPU_FIRMWARE_VERSION,
        }),
    }
}

#[must_use]
pub fn boot_image(info: &FirmwareInfo) -> FpgaBootImage {
    FpgaBootImage::from_u8(info.fpga_boot_image).unwrap_or(FpgaBootImage::Unknown)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateProgress {
    pub sent: usize,
    pub total: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceReply {
    pub status: u8,
    pub value: Vec<u8>,
}

pub struct Frame {
    pub cmd: Cmd,
    pub payload: [u8; PAYLOAD_BYTES],
    pub payload_len: usize,
}

impl Frame {
    #[must_use]
    pub fn new(cmd: Cmd) -> Self {
        Self {
            cmd,
            payload: [0; PAYLOAD_BYTES],
            payload_len: 0,
        }
    }

    #[must_use]
    pub fn with_payload(cmd: Cmd, header: &(impl IntoBytes + Immutable), data: &[u8]) -> Self {
        let mut frame = Self::new(cmd);
        let header = header.as_bytes();
        frame.payload[..header.len()].copy_from_slice(header);
        frame.payload[header.len()..][..data.len()].copy_from_slice(data);
        frame.payload_len = header.len() + data.len();
        frame
    }

    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(size_of::<FrameHeader>() + self.payload_len);
        bytes.extend_from_slice(
            FrameHeader {
                reserved: 0,
                cmd: self.cmd.as_u8(),
                exec_at: 0.into(),
            }
            .as_bytes(),
        );
        bytes.extend_from_slice(&self.payload[..self.payload_len]);
        bytes
    }
}

pub type Replies = Result<Vec<DeviceReply>, ()>;

pub trait Exchange {
    type Error: core::error::Error + Send + Sync + 'static;

    fn num_devices(&self) -> usize;

    fn exchange(&mut self, frame: &Frame) -> Result<Replies, Self::Error>;

    fn idle(&mut self, duration: Duration) -> Result<(), Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

pub struct UdpLink {
    client: Option<Arc<Client>>,
    states: StateChecker,
    lost_poll_interval: Duration,
    keepalive_stop: Arc<AtomicBool>,
    keepalive: Option<JoinHandle<()>>,
}

impl UdpLink {
    pub fn open(
        option: &TransportOption,
        num_devices: usize,
        heartbeat: Duration,
    ) -> Result<Self, Error> {
        let geometry = Geometry::new((0..num_devices).map(|_| Autd3::default()));
        let client = Arc::new(Client::open(&geometry, option)?);
        let keepalive_stop = Arc::new(AtomicBool::new(false));
        let keepalive = {
            let client = Arc::clone(&client);
            let stop = Arc::clone(&keepalive_stop);
            std::thread::Builder::new()
                .name("autd3-ota-keepalive".to_owned())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) && client.send_heartbeat().is_ok() {
                        std::thread::park_timeout(heartbeat);
                    }
                })
                .map_err(|e| Error::Network(NetworkCause::new(e)))?
        };
        Ok(Self {
            states: client.state_checker(),
            client: Some(client),
            lost_poll_interval: heartbeat,
            keepalive_stop,
            keepalive: Some(keepalive),
        })
    }

    fn stop_keepalive(&mut self) {
        self.keepalive_stop.store(true, Ordering::Release);
        if let Some(keepalive) = self.keepalive.take() {
            keepalive.thread().unpark();
            let _ = keepalive.join();
        }
    }

    fn any_lost(&self) -> Result<bool, Error> {
        Ok(self.states.check()?.any_lost())
    }

    fn wait_for_every_reply(&self, response: ResponseFuture) -> Result<Option<Response>, Error> {
        let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut response = pin!(response);
        loop {
            match response.as_mut().poll(&mut cx) {
                Poll::Ready(Ok(response)) => return Ok(Some(response)),
                Poll::Ready(Err(Error::Timeout { .. })) => return Ok(None),
                Poll::Ready(Err(e)) => return Err(e),
                Poll::Pending if self.any_lost()? => return Ok(None),
                Poll::Pending => std::thread::park_timeout(self.lost_poll_interval),
            }
        }
    }
}

impl Drop for UdpLink {
    fn drop(&mut self) {
        self.stop_keepalive();
    }
}

impl Exchange for UdpLink {
    type Error = Error;

    fn num_devices(&self) -> usize {
        self.client
            .as_ref()
            .map_or(0, |client| client.num_devices())
    }

    fn exchange(&mut self, frame: &Frame) -> Result<Replies, Error> {
        let client = self.client.as_ref().ok_or(Error::Closed)?;
        let datagrams = [Datagram {
            cmd: frame.cmd,
            payload: frame.payload,
            payload_len: frame.payload_len,
        }];
        let sent = pollster::block_on(
            client.send_frame(ClientFrame::new(Distribution::Broadcast, &datagrams)),
        );
        let response = match sent {
            Ok(response) => response,
            Err(Error::Timeout { .. }) => return Ok(Err(())),
            Err(e) => return Err(e),
        };
        let Some(response) = self.wait_for_every_reply(response)? else {
            return Ok(Err(()));
        };
        if self.any_lost()? {
            return Ok(Err(()));
        }
        Ok(Ok((0..response.error_codes().len())
            .map(|device| DeviceReply {
                status: response.error_codes()[device],
                value: response.reply_data_of(device).to_vec(),
            })
            .collect()))
    }

    fn idle(&mut self, duration: Duration) -> Result<(), Error> {
        std::thread::sleep(duration);
        Ok(())
    }

    fn close(&mut self) -> Result<(), Error> {
        self.stop_keepalive();
        self.client = None;
        Ok(())
    }
}

pub struct Driver<L: Exchange> {
    inner: L,
}

fn all_accepted(cmd: Cmd, replies: Vec<DeviceReply>) -> Result<Vec<DeviceReply>, DriverError> {
    match replies.iter().position(|reply| reply.status != 0) {
        None => Ok(replies),
        Some(device) => Err(DriverError::Device {
            device,
            cmd,
            code: replies[device].status,
        }),
    }
}

fn network_err<E: core::error::Error + Send + Sync + 'static>(e: E) -> DriverError {
    DriverError::Network(Box::new(e))
}

impl<L: Exchange> Driver<L> {
    pub fn new(inner: L) -> Self {
        Self { inner }
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn send(&mut self, frame: &Frame) -> Result<Vec<DeviceReply>, DriverError> {
        match self.inner.exchange(frame).map_err(network_err)? {
            Ok(replies) => Ok(replies),
            Err(()) => Err(DriverError::Timeout { cmd: frame.cmd }),
        }
    }

    pub fn send_checked(&mut self, frame: &Frame) -> Result<Vec<DeviceReply>, DriverError> {
        let replies = self.send(frame)?;
        all_accepted(frame.cmd, replies)
    }

    pub fn read_firmware_info(&mut self) -> Result<Vec<FirmwareInfo>, DriverError> {
        self.send_checked(&Frame::new(Cmd::ReadFirmwareInfo))?
            .into_iter()
            .enumerate()
            .map(|(device, reply)| {
                FirmwareInfo::read_from_prefix(&reply.value)
                    .map(|(info, _)| info)
                    .map_err(|_| DriverError::ShortReply {
                        device,
                        cmd: Cmd::ReadFirmwareInfo,
                    })
            })
            .collect()
    }

    pub fn read_running_image(&mut self) -> Result<Vec<Option<RunningImage>>, DriverError> {
        let cmd = Cmd::ReadRunningImage;
        self.send(&Frame::new(cmd))?
            .into_iter()
            .enumerate()
            .map(
                |(device, reply)| match autd3_cpu_wire::Error::from_u8(reply.status) {
                    Some(autd3_cpu_wire::Error::UnknownCmd) => Ok(None),
                    Some(autd3_cpu_wire::Error::None) => match reply.value.first() {
                        Some(&raw) => Ok(Some(
                            RunningImage::from_u8(raw).unwrap_or(RunningImage::Unknown),
                        )),
                        None => Err(DriverError::ShortReply { device, cmd }),
                    },
                    None
                    | Some(
                        autd3_cpu_wire::Error::InvalidPayload
                        | autd3_cpu_wire::Error::InvalidSilencerSetting
                        | autd3_cpu_wire::Error::InvalidTransitionMode
                        | autd3_cpu_wire::Error::MissTransitionTime
                        | autd3_cpu_wire::Error::FpgaTimeout
                        | autd3_cpu_wire::Error::SyncNotReady
                        | autd3_cpu_wire::Error::UpdateNotStarted
                        | autd3_cpu_wire::Error::UpdateImageInvalid
                        | autd3_cpu_wire::Error::UpdateFlash
                        | autd3_cpu_wire::Error::UpdateNotCommitted
                        | autd3_cpu_wire::Error::UpdateNothingToConfirm
                        | autd3_cpu_wire::Error::UpdateUnsupported
                        | autd3_cpu_wire::Error::FpgaUpdateInProgress
                        | autd3_cpu_wire::Error::UpdateActivating
                        | autd3_cpu_wire::Error::SyncMissed
                        | autd3_cpu_wire::Error::FailsafeConditionActive
                        | autd3_cpu_wire::Error::FpgaUnsupported,
                    ) => Err(DriverError::Device {
                        device,
                        cmd,
                        code: reply.status,
                    }),
                    Some(_) => unreachable!(),
                },
            )
            .collect()
    }

    pub fn ensure_fpga_update_supported(&mut self) -> Result<(), DriverError> {
        let infos = self.read_firmware_info()?;
        ensure_cpu_supported(&infos)?;
        match infos.iter().position(|info| {
            !FunctionBits::from_bits_retain(info.fpga_functions).contains(FunctionBits::FLASH_OTA)
        }) {
            None => Ok(()),
            Some(device) => Err(DriverError::FpgaUpdateUnsupported { device }),
        }
    }

    pub fn ensure_fpga_reconfigured(&mut self) -> Result<(), DriverError> {
        match self
            .read_firmware_info()?
            .iter()
            .position(|info| boot_image(info) == FpgaBootImage::ReconfigFailed)
        {
            None => Ok(()),
            Some(device) => Err(DriverError::FpgaReconfigFailed { device }),
        }
    }

    pub fn update_fpga(
        &mut self,
        image: &FpgaFirmwareImage,
        on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        self.ensure_fpga_update_supported()?;
        self.stream(
            image.as_bytes(),
            image.crc32(),
            [
                Cmd::FpgaUpdateBegin,
                Cmd::FpgaUpdateChunk,
                Cmd::FpgaUpdateCommit,
            ],
            on_progress,
        )
    }

    pub fn activate_fpga(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::FpgaUpdateActivate))
            .map(drop)
    }

    pub fn idle(&mut self, duration: Duration) -> Result<(), DriverError> {
        self.inner.idle(duration).map_err(network_err)
    }

    pub fn update(
        &mut self,
        image: &CpuFirmwareImage,
        on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        ensure_cpu_supported(&self.read_firmware_info()?)?;
        self.stream(
            image.as_bytes(),
            image.crc32(),
            [Cmd::UpdateBegin, Cmd::UpdateChunk, Cmd::UpdateCommit],
            on_progress,
        )
    }

    fn stream(
        &mut self,
        bytes: &[u8],
        crc32: u32,
        [begin_cmd, chunk_cmd, commit_cmd]: [Cmd; 3],
        mut on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        let total = bytes.len();
        let begin = Frame::with_payload(
            begin_cmd,
            &UpdateBeginPayload {
                length: U32::new(u32::try_from(total).expect("bounded by the slot capacity")),
                crc32: U32::new(crc32),
            },
            &[],
        );
        self.send_checked(&begin)?;
        on_progress(UpdateProgress { sent: 0, total });

        for (index, data) in bytes.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
            let offset = index * UPDATE_CHUNK_MAX_DATA_LEN;
            let chunk = Frame::with_payload(
                chunk_cmd,
                &UpdateChunkPayload {
                    offset: U32::new(u32::try_from(offset).expect("bounded by the slot capacity")),
                },
                data,
            );
            self.send_checked(&chunk)?;
            on_progress(UpdateProgress {
                sent: offset + data.len(),
                total,
            });
        }

        self.send_checked(&Frame::new(commit_cmd)).map(drop)
    }

    pub fn confirm(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::UpdateConfirm)).map(drop)
    }

    pub fn activate(&mut self) -> Result<(), DriverError> {
        let replies = self.send(&Frame::new(Cmd::UpdateActivate))?;
        all_accepted(Cmd::UpdateActivate, replies).map(drop)
    }

    pub fn activate_unrebooted(&mut self) -> Result<Vec<usize>, DriverError> {
        let replies = self.send(&Frame::new(Cmd::UpdateActivate))?;
        let mut activated = Vec::new();
        for (device, reply) in replies.iter().enumerate() {
            match autd3_cpu_wire::Error::from_u8(reply.status) {
                Some(autd3_cpu_wire::Error::None) => activated.push(device),
                Some(autd3_cpu_wire::Error::UpdateNotCommitted) => {}
                None
                | Some(
                    autd3_cpu_wire::Error::UnknownCmd
                    | autd3_cpu_wire::Error::InvalidPayload
                    | autd3_cpu_wire::Error::InvalidSilencerSetting
                    | autd3_cpu_wire::Error::InvalidTransitionMode
                    | autd3_cpu_wire::Error::MissTransitionTime
                    | autd3_cpu_wire::Error::FpgaTimeout
                    | autd3_cpu_wire::Error::SyncNotReady
                    | autd3_cpu_wire::Error::UpdateNotStarted
                    | autd3_cpu_wire::Error::UpdateImageInvalid
                    | autd3_cpu_wire::Error::UpdateFlash
                    | autd3_cpu_wire::Error::UpdateNothingToConfirm
                    | autd3_cpu_wire::Error::UpdateUnsupported
                    | autd3_cpu_wire::Error::FpgaUpdateInProgress
                    | autd3_cpu_wire::Error::UpdateActivating
                    | autd3_cpu_wire::Error::SyncMissed
                    | autd3_cpu_wire::Error::FailsafeConditionActive
                    | autd3_cpu_wire::Error::FpgaUnsupported,
                ) => {
                    return Err(DriverError::Device {
                        device,
                        cmd: Cmd::UpdateActivate,
                        code: reply.status,
                    });
                }
                Some(_) => unreachable!(),
            }
        }
        Ok(activated)
    }

    pub fn reboot(&mut self) -> Result<(), DriverError> {
        let replies = self.send(&Frame::new(Cmd::Reboot))?;
        all_accepted(Cmd::Reboot, replies).map(drop)
    }

    pub fn close(mut self) -> Result<(), DriverError> {
        self.inner.close().map_err(network_err)
    }

    #[must_use]
    pub fn into_inner(self) -> L {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running(cpu_versions: &[[u8; 3]]) -> Vec<FirmwareInfo> {
        cpu_versions
            .iter()
            .map(|&cpu_version| FirmwareInfo {
                cpu_version,
                fpga_version: [0; 3],
                fpga_functions: 0,
                fpga_boot_image: 0,
            })
            .collect()
    }

    struct Scripted(Vec<DeviceReply>);

    impl Exchange for Scripted {
        type Error = core::convert::Infallible;

        fn num_devices(&self) -> usize {
            self.0.len()
        }

        fn exchange(&mut self, _frame: &Frame) -> Result<Replies, Self::Error> {
            Ok(Ok(self.0.clone()))
        }

        fn idle(&mut self, _duration: Duration) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn reply(status: autd3_cpu_wire::Error, value: &[u8]) -> DeviceReply {
        DeviceReply {
            status: status.as_u8(),
            value: value.to_vec(),
        }
    }

    #[test]
    fn firmware_without_the_running_image_readout_reads_as_unreported() {
        let mut driver = Driver::new(Scripted(vec![
            reply(autd3_cpu_wire::Error::None, &[2]),
            reply(autd3_cpu_wire::Error::UnknownCmd, &[]),
            reply(autd3_cpu_wire::Error::None, &[1]),
            reply(autd3_cpu_wire::Error::None, &[0xFF]),
        ]));
        assert_eq!(
            driver.read_running_image().unwrap(),
            [
                Some(RunningImage::Unconfirmed),
                None,
                Some(RunningImage::Confirmed),
                Some(RunningImage::Unknown),
            ]
        );
    }

    #[test]
    fn a_failed_running_image_readout_is_an_error() {
        let mut driver = Driver::new(Scripted(vec![
            reply(autd3_cpu_wire::Error::None, &[2]),
            reply(autd3_cpu_wire::Error::UpdateFlash, &[]),
        ]));
        assert!(matches!(
            driver.read_running_image(),
            Err(DriverError::Device {
                device: 1,
                cmd: Cmd::ReadRunningImage,
                ..
            })
        ));
        let mut driver = Driver::new(Scripted(vec![reply(autd3_cpu_wire::Error::None, &[])]));
        assert!(matches!(
            driver.read_running_image(),
            Err(DriverError::ShortReply { device: 0, .. })
        ));
    }

    #[test]
    fn the_gate_names_the_first_device_below_the_minimum() {
        assert_eq!(first_unsupported(&[]), None);
        assert_eq!(first_unsupported(&running(&[[0, 10, 0], [1, 0, 0]])), None);
        assert_eq!(
            first_unsupported(&running(&[[0, 10, 0], [0, 9, 99], [0, 6, 1]])),
            Some((1, [0, 9, 99]))
        );
        assert_eq!(
            first_unsupported(&running(&[[0, 6, 1]])),
            Some((0, [0, 6, 1]))
        );
    }
}
