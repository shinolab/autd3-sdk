use std::time::{Duration, Instant};

use autd3_cpu_wire::describe_device_error;
use autd3_cpu_wire::fpga_params::FunctionBits;
use autd3_cpu_wire::fpga_update::FpgaBootImage;
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{FirmwareInfo, UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::update::RunningImage;
use autd3_rs::protocol::{Cmd, FrameHeader, PAYLOAD_BYTES, Seq};
use autd3_rs::{UdpBus, UdpError};
use zerocopy::little_endian::U32;
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::fpga_image::FpgaFirmwareImage;
use crate::image::CpuFirmwareImage;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);
const RETRANSMIT_INTERVAL: Duration = Duration::from_millis(100);
const RESET_RETRANSMIT_INTERVAL: Duration = Duration::from_millis(20);
const UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(4);
const UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(4);
const FPGA_UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(20);
const FPGA_UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(20);
const FPGA_UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
pub const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
const MIN_CPU_FIRMWARE_VERSION: [u8; 3] = [0, 10, 0];

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("network error: {0}")]
    Network(#[source] Box<dyn core::error::Error + Send + Sync>),
    #[error("device {device} did not acknowledge {cmd:?} within {timeout:?}; {}", timeout_hint(*cmd))]
    Timeout {
        device: usize,
        cmd: Cmd,
        timeout: Duration,
    },
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
    #[error("the devices did not acknowledge Reset")]
    ResetUnconfirmed,
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
        _ => "check the cable and the host firewall, then rerun",
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
        _ => "",
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
    pub fn bytes(&self, seq: Seq) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(size_of::<FrameHeader>() + self.payload_len);
        bytes.extend_from_slice(
            FrameHeader {
                seq: seq.get(),
                cmd: self.cmd.as_u8(),
            }
            .as_bytes(),
        );
        bytes.extend_from_slice(&self.payload[..self.payload_len]);
        bytes
    }
}

pub type Replies = Result<Vec<DeviceReply>, usize>;

pub trait Exchange {
    type Error: core::error::Error + Send + Sync + 'static;

    fn num_devices(&self) -> usize;

    fn reset(&mut self, timeout: Duration) -> Result<bool, Self::Error>;

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        timeout: Duration,
        retransmit: Duration,
    ) -> Result<Replies, Self::Error>;

    fn idle(&mut self, duration: Duration) -> Result<(), Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn not_before(msg_id: u16, first: u16) -> bool {
    msg_id.wrapping_sub(first) < 0x8000
}

impl Exchange for UdpBus {
    type Error = UdpError;

    fn num_devices(&self) -> usize {
        UdpBus::num_devices(self)
    }

    fn reset(&mut self, timeout: Duration) -> Result<bool, UdpError> {
        let frame = Frame::new(Cmd::Reset);
        let frames = vec![frame.bytes(Seq::ZERO); self.num_devices()];
        let first = self.next_msg_id();
        let start = Instant::now();
        let mut confirmed = vec![false; self.num_devices()];
        while start.elapsed() < timeout {
            self.send(&frames)?;
            let deadline = (Instant::now() + RETRANSMIT_INTERVAL).min(start + timeout);
            while confirmed.contains(&false) {
                let Some(reply) = self.recv(deadline)? else {
                    break;
                };
                if not_before(reply.msg_id, first) && reply.ack == 0xFF {
                    confirmed[reply.device] = true;
                }
            }
            if !confirmed.contains(&false) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        timeout: Duration,
        retransmit: Duration,
    ) -> Result<Replies, UdpError> {
        let frames = vec![frame.bytes(seq); self.num_devices()];
        let mut replies: Vec<Option<DeviceReply>> = vec![None; self.num_devices()];
        let first = self.next_msg_id();
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            self.send(&frames)?;
            let retransmit_at = (Instant::now() + retransmit).min(end);
            loop {
                let keepalive = (Instant::now()
                    + self.heartbeat_interval().unwrap_or(RETRANSMIT_INTERVAL))
                .min(retransmit_at);
                let Some(reply) = self.recv(keepalive)? else {
                    if Instant::now() >= retransmit_at {
                        break;
                    }
                    self.heartbeat()?;
                    continue;
                };
                if not_before(reply.msg_id, first)
                    && reply.ack == seq.get()
                    && replies[reply.device].is_none()
                {
                    replies[reply.device] = Some(DeviceReply {
                        status: reply.status,
                        value: reply.data().to_vec(),
                    });
                }
                if replies.iter().all(Option::is_some) {
                    return Ok(Ok(replies.into_iter().map(Option::unwrap).collect()));
                }
            }
        }
        Ok(Err(replies.iter().position(Option::is_none).unwrap_or(0)))
    }

    fn idle(&mut self, duration: Duration) -> Result<(), UdpError> {
        let start = Instant::now();
        while start.elapsed() < duration {
            self.heartbeat()?;
            let deadline = (Instant::now()
                + self.heartbeat_interval().unwrap_or(RETRANSMIT_INTERVAL))
            .min(start + duration);
            while self.recv(deadline)?.is_some() {}
        }
        Ok(())
    }

    fn close(&mut self) -> Result<(), UdpError> {
        UdpBus::close(self)
    }
}

pub struct Driver<L: Exchange> {
    inner: L,
    next_seq: Seq,
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
    pub fn open(inner: L) -> Result<Self, DriverError> {
        let mut driver = Self {
            inner,
            next_seq: Seq::ZERO,
        };
        driver.handshake()?;
        Ok(driver)
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.inner.num_devices()
    }

    fn handshake(&mut self) -> Result<(), DriverError> {
        if !self.inner.reset(DEFAULT_TIMEOUT).map_err(network_err)? {
            return Err(DriverError::ResetUnconfirmed);
        }
        self.next_seq = Seq::ZERO;
        Ok(())
    }

    fn send(&mut self, frame: &Frame, timeout: Duration) -> Result<Vec<DeviceReply>, DriverError> {
        self.send_every(frame, timeout, RETRANSMIT_INTERVAL)
    }

    fn send_every(
        &mut self,
        frame: &Frame,
        timeout: Duration,
        retransmit: Duration,
    ) -> Result<Vec<DeviceReply>, DriverError> {
        let seq = self.next_seq;
        match self
            .inner
            .exchange(seq, frame, timeout, retransmit)
            .map_err(network_err)?
        {
            Ok(replies) => {
                self.next_seq = seq.next();
                Ok(replies)
            }
            Err(device) => Err(DriverError::Timeout {
                device,
                cmd: frame.cmd,
                timeout,
            }),
        }
    }

    pub fn send_checked(
        &mut self,
        frame: &Frame,
        timeout: Duration,
    ) -> Result<Vec<DeviceReply>, DriverError> {
        let replies = self.send(frame, timeout)?;
        all_accepted(frame.cmd, replies)
    }

    fn send_reset_request(&mut self, cmd: Cmd) -> Result<Vec<DeviceReply>, DriverError> {
        self.send_every(&Frame::new(cmd), DEFAULT_TIMEOUT, RESET_RETRANSMIT_INTERVAL)
    }

    pub fn read_firmware_info(&mut self) -> Result<Vec<FirmwareInfo>, DriverError> {
        self.send_checked(&Frame::new(Cmd::ReadFirmwareInfo), DEFAULT_TIMEOUT)?
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
        self.send(&Frame::new(cmd), DEFAULT_TIMEOUT)?
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
                    _ => Err(DriverError::Device {
                        device,
                        cmd,
                        code: reply.status,
                    }),
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
            [
                FPGA_UPDATE_BEGIN_TIMEOUT,
                FPGA_UPDATE_CHUNK_TIMEOUT,
                FPGA_UPDATE_COMMIT_TIMEOUT,
            ],
            on_progress,
        )
    }

    pub fn activate_fpga(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::FpgaUpdateActivate), DEFAULT_TIMEOUT)
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
            [
                UPDATE_BEGIN_TIMEOUT,
                UPDATE_CHUNK_TIMEOUT,
                UPDATE_COMMIT_TIMEOUT,
            ],
            on_progress,
        )
    }

    fn stream(
        &mut self,
        bytes: &[u8],
        crc32: u32,
        [begin_cmd, chunk_cmd, commit_cmd]: [Cmd; 3],
        [begin_timeout, chunk_timeout, commit_timeout]: [Duration; 3],
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
        self.send_checked(&begin, begin_timeout)?;
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
            self.send_checked(&chunk, chunk_timeout)?;
            on_progress(UpdateProgress {
                sent: offset + data.len(),
                total,
            });
        }

        self.send_checked(&Frame::new(commit_cmd), commit_timeout)
            .map(drop)
    }

    pub fn confirm(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::UpdateConfirm), UPDATE_CONFIRM_TIMEOUT)
            .map(drop)
    }

    pub fn activate(&mut self) -> Result<(), DriverError> {
        let replies = self.send_reset_request(Cmd::UpdateActivate)?;
        all_accepted(Cmd::UpdateActivate, replies).map(drop)
    }

    pub fn activate_unrebooted(&mut self) -> Result<Vec<usize>, DriverError> {
        let replies = self.send_reset_request(Cmd::UpdateActivate)?;
        let mut activated = Vec::new();
        for (device, reply) in replies.iter().enumerate() {
            match autd3_cpu_wire::Error::from_u8(reply.status) {
                Some(autd3_cpu_wire::Error::None) => activated.push(device),
                Some(autd3_cpu_wire::Error::UpdateNotCommitted) => {}
                _ => {
                    return Err(DriverError::Device {
                        device,
                        cmd: Cmd::UpdateActivate,
                        code: reply.status,
                    });
                }
            }
        }
        Ok(activated)
    }

    pub fn reboot(&mut self) -> Result<(), DriverError> {
        let replies = self.send_reset_request(Cmd::Reboot)?;
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

        fn reset(&mut self, _timeout: Duration) -> Result<bool, Self::Error> {
            Ok(true)
        }

        fn exchange(
            &mut self,
            _seq: Seq,
            _frame: &Frame,
            _timeout: Duration,
            _retransmit: Duration,
        ) -> Result<Replies, Self::Error> {
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
        let mut driver = Driver::open(Scripted(vec![
            reply(autd3_cpu_wire::Error::None, &[2]),
            reply(autd3_cpu_wire::Error::UnknownCmd, &[]),
            reply(autd3_cpu_wire::Error::None, &[1]),
            reply(autd3_cpu_wire::Error::None, &[0xFF]),
        ]))
        .unwrap();
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
        let mut driver = Driver::open(Scripted(vec![
            reply(autd3_cpu_wire::Error::None, &[2]),
            reply(autd3_cpu_wire::Error::UpdateFlash, &[]),
        ]))
        .unwrap();
        assert!(matches!(
            driver.read_running_image(),
            Err(DriverError::Device {
                device: 1,
                cmd: Cmd::ReadRunningImage,
                ..
            })
        ));
        let mut driver =
            Driver::open(Scripted(vec![reply(autd3_cpu_wire::Error::None, &[])])).unwrap();
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
