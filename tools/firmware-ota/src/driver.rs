use std::time::{Duration, Instant};

use autd3_cpu_wire::fpga_update::{FPGA_FUNC_FLASH_OTA, FpgaBootImage};
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{
    FirmwareInfo, SetModePayload, UpdateBeginPayload, UpdateChunkPayload,
};
use autd3_cpu_wire::{Mode, describe_device_error};
use autd3_rs::protocol::{Cmd, FRAME_HEADER_BYTES, PAYLOAD_BYTES, Seq};
use autd3_rs::{UdpBus, UdpError};
use zerocopy::little_endian::{U16, U32};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga_image::FpgaFirmwareImage;
use crate::image::CpuFirmwareImage;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);
pub const RETRANSMIT_INTERVAL: Duration = Duration::from_millis(100);
pub const UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(30);
pub const UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(4);
pub const UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(30);
pub const UPDATE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(4);
pub const FPGA_UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(20);
pub const FPGA_UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(20);
pub const FPGA_UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
pub const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
pub const MIN_CPU_FIRMWARE_VERSION: (u8, u8, u8) = (0, 10, 0);

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("network error: {0}")]
    Network(#[source] Box<dyn core::error::Error + Send + Sync>),
    #[error("device {device} did not acknowledge {cmd} within {timeout:?}; {}", timeout_hint(*cmd))]
    Timeout {
        device: usize,
        cmd: Request,
        timeout: Duration,
    },
    #[error("device {device} rejected {cmd} with firmware error {code:#04x}: {}{}", describe_device_error(*code), device_hint(*cmd, *code))]
    Device {
        device: usize,
        cmd: Request,
        code: u8,
    },
    #[error("device {device} returned a shorter reply to {cmd} than the command defines")]
    ShortReply { device: usize, cmd: Request },
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
    #[error("mode negotiation failed: the devices did not acknowledge SetMode")]
    ModeNegotiation,
    #[error(
        "device {device} runs CPU firmware {}.{}.{}, but the update needs {}.{}.{} or newer; flash it once via J-Link",
        found.0, found.1, found.2, required.0, required.1, required.2
    )]
    UnsupportedFirmware {
        device: usize,
        found: (u8, u8, u8),
        required: (u8, u8, u8),
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    Command(Cmd),
    Legacy(u8),
}

impl Request {
    #[must_use]
    pub const fn id(self) -> u8 {
        match self {
            Self::Command(cmd) => cmd.as_u8(),
            Self::Legacy(id) => id,
        }
    }
}

impl From<Cmd> for Request {
    fn from(cmd: Cmd) -> Self {
        Self::Command(cmd)
    }
}

impl core::fmt::Display for Request {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Command(cmd) => write!(f, "{cmd:?}"),
            Self::Legacy(id) => write!(f, "v0.9 read command {id:#04x}"),
        }
    }
}

fn timeout_hint(cmd: Request) -> &'static str {
    let Request::Command(cmd) = cmd else {
        return "check the cable and the host firewall, then rerun";
    };
    match cmd {
        Cmd::UpdateBegin | Cmd::UpdateChunk | Cmd::UpdateCommit => {
            "the device keeps the half-written slot and boots its previous image; rerun the update from the beginning"
        }
        Cmd::FpgaUpdateBegin | Cmd::FpgaUpdateChunk | Cmd::FpgaUpdateCommit => {
            "the device stays locked with its output off until the next power cycle; power-cycle it and rerun the FPGA update"
        }
        _ => "check the cable and the host firewall, then rerun",
    }
}

fn device_hint(cmd: Request, code: u8) -> &'static str {
    let Request::Command(cmd) = cmd else {
        return "";
    };
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

#[must_use]
pub fn first_unsupported(
    versions: &[(u8, u8, u8)],
    min: (u8, u8, u8),
) -> Option<(usize, (u8, u8, u8))> {
    versions
        .iter()
        .copied()
        .enumerate()
        .find(|&(_, version)| version < min)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateProgress {
    pub sent: usize,
    pub total: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dialect {
    #[default]
    Udp,
    Legacy,
}

pub const LEGACY_PAYLOAD_BYTES: usize = 624;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct LegacyUpdateChunkPayload {
    pub offset: U32,
    pub data_len: U16,
}

const _: () = assert!(core::mem::size_of::<LegacyUpdateChunkPayload>() == 6);

const LEGACY_READ_ERROR_DETAIL: u8 = 0xE0;
const LEGACY_ERR_FPGA_RECONFIG_FAILED: u8 = 0x11;
const LEGACY_READ_CPU_VERSION: [u8; 3] = [0xE1, 0xE2, 0xE3];
const LEGACY_READ_FPGA_VERSION: [u8; 3] = [0xE4, 0xE5, 0xE6];
const LEGACY_READ_FPGA_FUNCTIONS: u8 = 0xE9;
const LEGACY_READ_FPGA_BOOT_IMAGE: u8 = 0xEA;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceReply {
    pub status: u8,
    pub value: Vec<u8>,
}

pub struct Frame {
    pub cmd: u8,
    pub payload: [u8; PAYLOAD_BYTES],
    pub payload_len: usize,
}

impl Frame {
    #[must_use]
    pub fn new(cmd: impl Into<Request>) -> Self {
        Self {
            cmd: cmd.into().id(),
            payload: [0; PAYLOAD_BYTES],
            payload_len: 0,
        }
    }

    #[must_use]
    pub fn with_payload(
        cmd: impl Into<Request>,
        header: &(impl IntoBytes + Immutable),
        data: &[u8],
    ) -> Self {
        let mut frame = Self::new(cmd);
        let header = header.as_bytes();
        frame.payload[..header.len()].copy_from_slice(header);
        frame.payload[header.len()..header.len() + data.len()].copy_from_slice(data);
        frame.payload_len = header.len() + data.len();
        frame
    }

    #[must_use]
    pub fn bytes(&self, seq: Seq) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FRAME_HEADER_BYTES + self.payload_len);
        bytes.push(seq.get());
        bytes.push(self.cmd);
        bytes.extend_from_slice(&self.payload[..self.payload_len]);
        bytes
    }
}

pub type Replies = Result<Vec<DeviceReply>, usize>;

pub trait Exchange {
    type Error: core::error::Error + Send + Sync + 'static;

    fn num_devices(&self) -> usize;

    fn min_cpu_firmware_version(&self) -> (u8, u8, u8) {
        MIN_CPU_FIRMWARE_VERSION
    }

    fn dialect(&self) -> Dialect {
        Dialect::Udp
    }

    fn reset(&mut self, timeout: Duration) -> Result<bool, Self::Error>;

    fn exchange(
        &mut self,
        seq: Seq,
        frame: &Frame,
        timeout: Duration,
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
    ) -> Result<Replies, UdpError> {
        let frames = vec![frame.bytes(seq); self.num_devices()];
        let mut replies: Vec<Option<DeviceReply>> = vec![None; self.num_devices()];
        let first = self.next_msg_id();
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            self.send(&frames)?;
            let retransmit_at = (Instant::now() + RETRANSMIT_INTERVAL).min(end);
            loop {
                let keepalive = (Instant::now() + self.heartbeat_interval()).min(retransmit_at);
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
            let deadline = (Instant::now() + self.heartbeat_interval()).min(start + duration);
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
        let frame = Frame::with_payload(Cmd::SetMode, &SetModePayload { mode: Mode::Fifo }, &[]);
        match self
            .inner
            .exchange(Seq::ZERO, &frame, DEFAULT_TIMEOUT)
            .map_err(network_err)?
        {
            Ok(_) => {
                self.next_seq = Seq::new(1);
                Ok(())
            }
            Err(_) => Err(DriverError::ModeNegotiation),
        }
    }

    pub fn send(
        &mut self,
        request: impl Into<Request>,
        frame: &Frame,
        timeout: Duration,
    ) -> Result<Vec<DeviceReply>, DriverError> {
        let seq = self.next_seq;
        match self
            .inner
            .exchange(seq, frame, timeout)
            .map_err(network_err)?
        {
            Ok(replies) => {
                self.next_seq = seq.next();
                Ok(replies)
            }
            Err(device) => Err(DriverError::Timeout {
                device,
                cmd: request.into(),
                timeout,
            }),
        }
    }

    pub fn send_checked(&mut self, frame: &Frame, timeout: Duration) -> Result<(), DriverError> {
        let cmd = Self::request_of(frame);
        let replies = self.send(cmd, frame, timeout)?;
        match replies.iter().position(|reply| reply.status != 0) {
            None => Ok(()),
            Some(device) => Err(DriverError::Device {
                device,
                cmd,
                code: replies[device].status,
            }),
        }
    }

    fn request_of(frame: &Frame) -> Request {
        Cmd::from_u8(frame.cmd).map_or(Request::Legacy(frame.cmd), Request::Command)
    }

    fn legacy_read(&mut self, id: u8) -> Result<Vec<u8>, DriverError> {
        let replies = self.send(
            Request::Legacy(id),
            &Frame::new(Request::Legacy(id)),
            DEFAULT_TIMEOUT,
        )?;
        Ok(replies.into_iter().map(|reply| reply.status).collect())
    }

    fn legacy_triple(&mut self, ids: [u8; 3]) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        let [major, minor, patch] = ids.map(|id| self.legacy_read(id));
        Ok(major?
            .into_iter()
            .zip(minor?)
            .zip(patch?)
            .map(|((major, minor), patch)| (major, minor, patch))
            .collect())
    }

    fn read(&mut self, cmd: Cmd) -> Result<Vec<Vec<u8>>, DriverError> {
        let replies = self.send(cmd, &Frame::new(cmd), DEFAULT_TIMEOUT)?;
        if let Some(device) = replies.iter().position(|reply| reply.status != 0) {
            return Err(DriverError::Device {
                device,
                cmd: cmd.into(),
                code: replies[device].status,
            });
        }
        Ok(replies.into_iter().map(|reply| reply.value).collect())
    }

    fn read_firmware_info(&mut self) -> Result<Vec<FirmwareInfo>, DriverError> {
        self.read(Cmd::ReadFirmwareInfo)?
            .into_iter()
            .enumerate()
            .map(|(device, value)| {
                FirmwareInfo::read_from_prefix(&value)
                    .map(|(info, _)| info)
                    .map_err(|_| DriverError::ShortReply {
                        device,
                        cmd: Cmd::ReadFirmwareInfo.into(),
                    })
            })
            .collect()
    }

    pub fn read_cpu_version(&mut self) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        match self.inner.dialect() {
            Dialect::Legacy => self.legacy_triple(LEGACY_READ_CPU_VERSION),
            Dialect::Udp => Ok(self
                .read_firmware_info()?
                .into_iter()
                .map(|info| {
                    let [major, minor, patch] = info.cpu_version;
                    (major, minor, patch)
                })
                .collect()),
        }
    }

    pub fn read_fpga_version(&mut self) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        match self.inner.dialect() {
            Dialect::Legacy => self.legacy_triple(LEGACY_READ_FPGA_VERSION),
            Dialect::Udp => Ok(self
                .read_firmware_info()?
                .into_iter()
                .map(|info| {
                    let [major, minor, patch] = info.fpga_version;
                    (major, minor, patch)
                })
                .collect()),
        }
    }

    pub fn read_fpga_functions(&mut self) -> Result<Vec<u8>, DriverError> {
        match self.inner.dialect() {
            Dialect::Legacy => self.legacy_read(LEGACY_READ_FPGA_FUNCTIONS),
            Dialect::Udp => Ok(self
                .read_firmware_info()?
                .into_iter()
                .map(|info| info.fpga_functions)
                .collect()),
        }
    }

    pub fn read_fpga_boot_image(&mut self) -> Result<Vec<FpgaBootImage>, DriverError> {
        if self.inner.dialect() == Dialect::Udp {
            return Ok(self
                .read_firmware_info()?
                .into_iter()
                .map(|info| {
                    FpgaBootImage::from_u8(info.fpga_boot_image).unwrap_or(FpgaBootImage::Unknown)
                })
                .collect());
        }
        let unknown_cmd = autd3_cpu_wire::Error::UnknownCmd.as_u8();
        let before = self.legacy_read(LEGACY_READ_ERROR_DETAIL)?;
        let raw = self.legacy_read(LEGACY_READ_FPGA_BOOT_IMAGE)?;
        let after = self.legacy_read(LEGACY_READ_ERROR_DETAIL)?;
        Ok(raw
            .into_iter()
            .zip(before.into_iter().zip(after))
            .map(|(raw, (before, after))| {
                if after == unknown_cmd && (before != unknown_cmd || raw == unknown_cmd) {
                    FpgaBootImage::Unknown
                } else {
                    FpgaBootImage::from_u8(raw).unwrap_or(FpgaBootImage::Unknown)
                }
            })
            .collect())
    }

    pub fn ensure_fpga_update_supported(&mut self) -> Result<(), DriverError> {
        self.ensure_update_supported()?;
        match self
            .read_fpga_functions()?
            .iter()
            .position(|functions| functions & FPGA_FUNC_FLASH_OTA == 0)
        {
            None => Ok(()),
            Some(device) => Err(DriverError::FpgaUpdateUnsupported { device }),
        }
    }

    pub fn ensure_fpga_reconfigured(&mut self) -> Result<(), DriverError> {
        let failed = match self.inner.dialect() {
            Dialect::Legacy => self
                .legacy_read(LEGACY_READ_ERROR_DETAIL)?
                .iter()
                .position(|&detail| detail == LEGACY_ERR_FPGA_RECONFIG_FAILED),
            Dialect::Udp => self
                .read_fpga_boot_image()?
                .iter()
                .position(|&image| image == FpgaBootImage::ReconfigFailed),
        };
        match failed {
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
    }

    pub fn idle(&mut self, duration: Duration) -> Result<(), DriverError> {
        self.inner.idle(duration).map_err(network_err)
    }

    pub fn update(
        &mut self,
        image: &CpuFirmwareImage,
        on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        self.ensure_update_supported()?;
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

    fn chunk_len(&self) -> usize {
        match self.inner.dialect() {
            Dialect::Udp => UPDATE_CHUNK_MAX_DATA_LEN,
            Dialect::Legacy => LEGACY_PAYLOAD_BYTES - size_of::<LegacyUpdateChunkPayload>(),
        }
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

        let chunk_len = self.chunk_len();
        for (index, data) in bytes.chunks(chunk_len).enumerate() {
            let offset = index * chunk_len;
            let offset_field =
                U32::new(u32::try_from(offset).expect("bounded by the slot capacity"));
            let chunk = match self.inner.dialect() {
                Dialect::Udp => Frame::with_payload(
                    chunk_cmd,
                    &UpdateChunkPayload {
                        offset: offset_field,
                    },
                    data,
                ),
                Dialect::Legacy => Frame::with_payload(
                    chunk_cmd,
                    &LegacyUpdateChunkPayload {
                        offset: offset_field,
                        data_len: U16::new(
                            u16::try_from(data.len()).expect("bounded by the chunk size"),
                        ),
                    },
                    data,
                ),
            };
            self.send_checked(&chunk, chunk_timeout)?;
            on_progress(UpdateProgress {
                sent: offset + data.len(),
                total,
            });
        }

        self.send_checked(&Frame::new(commit_cmd), commit_timeout)
    }

    pub fn ensure_update_supported(&mut self) -> Result<(), DriverError> {
        let required = self.inner.min_cpu_firmware_version();
        match first_unsupported(&self.read_cpu_version()?, required) {
            None => Ok(()),
            Some((device, found)) => Err(DriverError::UnsupportedFirmware {
                device,
                found,
                required,
            }),
        }
    }

    pub fn confirm(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::UpdateConfirm), UPDATE_CONFIRM_TIMEOUT)
    }

    pub fn activate(&mut self) -> Result<(), DriverError> {
        self.send_checked(&Frame::new(Cmd::UpdateActivate), DEFAULT_TIMEOUT)
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

    #[test]
    fn the_gate_names_the_first_device_below_the_minimum() {
        let min = MIN_CPU_FIRMWARE_VERSION;
        assert_eq!(first_unsupported(&[], min), None);
        assert_eq!(first_unsupported(&[(0, 10, 0), (1, 0, 0)], min), None);
        assert_eq!(
            first_unsupported(&[(0, 10, 0), (0, 9, 99), (0, 6, 1)], min),
            Some((1, (0, 9, 99)))
        );
        assert_eq!(first_unsupported(&[(0, 6, 1)], min), Some((0, (0, 6, 1))));
        assert_eq!(first_unsupported(&[(0, 9, 0)], (0, 9, 0)), None);
    }
}
