use std::time::{Duration, Instant};

use autd3_cpu_wire::fpga_update::{FPGA_FUNC_FLASH_OTA, FpgaBootImage};
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{SetModePayload, UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::{Mode, describe_device_error};
use autd3_rs_core::link::Link;
use autd3_rs_core::protocol::{Cmd, RX_FRAME_BYTES, RxFrame, Seq, TX_FRAME_BYTES, TxFrame};
use zerocopy::FromBytes;

use crate::fpga_image::FpgaFirmwareImage;
use crate::image::CpuFirmwareImage;

pub const RESET_CYCLES: u32 = 2;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(1);
pub const UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(30);
pub const UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(4);
pub const UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(30);
pub const UPDATE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(4);
pub const FPGA_UPDATE_BEGIN_TIMEOUT: Duration = Duration::from_secs(20);
pub const FPGA_UPDATE_CHUNK_TIMEOUT: Duration = Duration::from_secs(20);
pub const FPGA_UPDATE_COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
pub const FPGA_RECONFIG_WAIT: Duration =
    Duration::from_millis(autd3_cpu_wire::fpga_update::FPGA_RECONFIG_WORST_MS as u64 + 2000);
pub const MIN_CPU_FIRMWARE_VERSION: (u8, u8, u8) = (0, 9, 0);

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("link error: {0}")]
    Link(#[source] Box<dyn core::error::Error + Send + Sync>),
    #[error("device {device} did not acknowledge {cmd:?} within {timeout:?}; {}", timeout_hint(*cmd))]
    Timeout {
        device: usize,
        cmd: Cmd,
        timeout: Duration,
    },
    #[error("device {device} rejected {cmd:?} with firmware error {code:#04x}: {}{}", describe_device_error(*code), device_hint(*cmd, *code))]
    Device { device: usize, cmd: Cmd, code: u8 },
    #[error(
        "device {device} runs an FPGA image without configuration-flash access; flash `autd3-fpga.mcs` once via JTAG"
    )]
    FpgaUpdateUnsupported { device: usize },
    #[error(
        "device {device} did not reconfigure after activation (or an earlier attempt failed since power-on); the written image boots at the next power cycle"
    )]
    FpgaReconfigFailed { device: usize },
    #[error("mode negotiation failed: the devices did not acknowledge SetMode")]
    ModeNegotiation,
    #[error(
        "device {device} runs CPU firmware {}.{}.{}, but EtherCAT update needs {}.{}.{} or newer; flash it once via J-Link",
        found.0, found.1, found.2, required.0, required.1, required.2
    )]
    UnsupportedFirmware {
        device: usize,
        found: (u8, u8, u8),
        required: (u8, u8, u8),
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
        _ => "check the link (cable, master state) and rerun",
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

#[must_use]
pub fn first_unsupported(versions: &[(u8, u8, u8)]) -> Option<(usize, (u8, u8, u8))> {
    versions
        .iter()
        .copied()
        .enumerate()
        .find(|&(_, version)| version < MIN_CPU_FIRMWARE_VERSION)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UpdateProgress {
    pub sent: usize,
    pub total: usize,
}

pub struct Driver<L: Link> {
    link: L,
    tx: Vec<[u8; TX_FRAME_BYTES]>,
    rx: Vec<[u8; RX_FRAME_BYTES]>,
    next_seq: Seq,
}

fn link_err<E: core::error::Error + Send + Sync + 'static>(e: E) -> DriverError {
    DriverError::Link(Box::new(e))
}

impl<L: Link> Driver<L> {
    pub fn open(link: L) -> Result<Self, DriverError> {
        let n = link.num_devices();
        let mut driver = Self {
            link,
            tx: vec![[0; TX_FRAME_BYTES]; n],
            rx: vec![[0; RX_FRAME_BYTES]; n],
            next_seq: Seq::ZERO,
        };
        driver.handshake()?;
        Ok(driver)
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.tx.len()
    }

    fn cycle(&mut self) -> Result<bool, DriverError> {
        self.link.wait_next_cycle();
        self.link
            .cycle(&self.tx, &mut self.rx)
            .map(autd3_rs_core::CycleOutcome::rx_valid)
            .map_err(link_err)
    }

    fn stage(&mut self, frame: &TxFrame) {
        for buf in &mut self.tx {
            frame.write_to(buf);
        }
    }

    fn handshake(&mut self) -> Result<(), DriverError> {
        self.stage(&TxFrame::new(Seq::ZERO, Cmd::Reset));
        for _ in 0..RESET_CYCLES {
            self.cycle()?;
        }
        let mut frame = TxFrame::new(Seq::ZERO, Cmd::SetMode);
        let (p, _) = SetModePayload::mut_from_prefix(&mut frame.payload).unwrap();
        p.mode = Mode::Fifo.as_u8();
        self.stage(&frame);
        let start = Instant::now();
        loop {
            let valid = self.cycle()?;
            if valid && self.rx.iter().all(|rx| RxFrame::parse(rx).ack == Seq::ZERO) {
                self.next_seq = Seq::new(1);
                return Ok(());
            }
            if start.elapsed() >= DEFAULT_TIMEOUT {
                return Err(DriverError::ModeNegotiation);
            }
        }
    }

    pub fn send(&mut self, mut frame: TxFrame, timeout: Duration) -> Result<Vec<u8>, DriverError> {
        let seq = self.next_seq;
        frame.seq = seq;
        self.stage(&frame);
        let mut data = vec![None; self.num_devices()];
        let start = Instant::now();
        loop {
            if self.cycle()? {
                for (slot, rx) in data.iter_mut().zip(&self.rx) {
                    let rx = RxFrame::parse(rx);
                    if slot.is_none() && rx.ack == seq {
                        *slot = Some(rx.data);
                    }
                }
                if data.iter().all(Option::is_some) {
                    self.next_seq = seq.next();
                    return Ok(data.into_iter().map(Option::unwrap).collect());
                }
            }
            if start.elapsed() >= timeout {
                break;
            }
        }
        let device = data.iter().position(Option::is_none).unwrap_or(0);
        Err(DriverError::Timeout {
            device,
            cmd: frame.cmd,
            timeout,
        })
    }

    pub fn send_checked(&mut self, frame: TxFrame, timeout: Duration) -> Result<(), DriverError> {
        let cmd = frame.cmd;
        let data = self.send(frame, timeout)?;
        match data.iter().position(|&code| code != 0) {
            None => Ok(()),
            Some(device) => Err(DriverError::Device {
                device,
                cmd,
                code: data[device],
            }),
        }
    }

    pub fn read_cpu_version(&mut self) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        self.read_triple([
            Cmd::ReadCpuFwVersionMajor,
            Cmd::ReadCpuFwVersionMinor,
            Cmd::ReadCpuFwVersionPatch,
        ])
    }

    fn read_triple(&mut self, cmds: [Cmd; 3]) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        let [major, minor, patch] = cmds;
        let major = self.send(TxFrame::new(Seq::ZERO, major), DEFAULT_TIMEOUT)?;
        let minor = self.send(TxFrame::new(Seq::ZERO, minor), DEFAULT_TIMEOUT)?;
        let patch = self.send(TxFrame::new(Seq::ZERO, patch), DEFAULT_TIMEOUT)?;
        Ok(major
            .into_iter()
            .zip(minor)
            .zip(patch)
            .map(|((major, minor), patch)| (major, minor, patch))
            .collect())
    }

    pub fn read_fpga_version(&mut self) -> Result<Vec<(u8, u8, u8)>, DriverError> {
        self.read_triple([
            Cmd::ReadFpgaFwVersionMajor,
            Cmd::ReadFpgaFwVersionMinor,
            Cmd::ReadFpgaFwVersionPatch,
        ])
    }

    pub fn read_fpga_functions(&mut self) -> Result<Vec<u8>, DriverError> {
        self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadFpgaFunctions),
            DEFAULT_TIMEOUT,
        )
    }

    fn read_error_detail(&mut self) -> Result<Vec<u8>, DriverError> {
        self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadErrorDetail),
            DEFAULT_TIMEOUT,
        )
    }

    pub fn read_fpga_boot_image(&mut self) -> Result<Vec<FpgaBootImage>, DriverError> {
        let unknown_cmd = autd3_cpu_wire::Error::UnknownCmd.as_u8();
        let before = self.read_error_detail()?;
        let raw = self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadFpgaBootImage),
            DEFAULT_TIMEOUT,
        )?;
        let after = self.read_error_detail()?;
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
        let failed = autd3_cpu_wire::Error::FpgaReconfigFailed.as_u8();
        match self
            .read_error_detail()?
            .iter()
            .position(|&detail| detail == failed)
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
        self.send_checked(
            TxFrame::new(Seq::ZERO, Cmd::FpgaUpdateActivate),
            DEFAULT_TIMEOUT,
        )
    }

    pub fn idle(&mut self, duration: Duration) -> Result<(), DriverError> {
        let start = Instant::now();
        while start.elapsed() < duration {
            self.cycle()?;
        }
        Ok(())
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

    fn stream(
        &mut self,
        bytes: &[u8],
        crc32: u32,
        [begin_cmd, chunk_cmd, commit_cmd]: [Cmd; 3],
        [begin_timeout, chunk_timeout, commit_timeout]: [Duration; 3],
        mut on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        let total = bytes.len();
        let mut begin = TxFrame::new(Seq::ZERO, begin_cmd);
        let (p, _) = UpdateBeginPayload::mut_from_prefix(&mut begin.payload).unwrap();
        p.length
            .set(u32::try_from(total).expect("bounded by the slot capacity"));
        p.crc32.set(crc32);
        self.send_checked(begin, begin_timeout)?;
        on_progress(UpdateProgress { sent: 0, total });

        for (index, data) in bytes.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
            let offset = index * UPDATE_CHUNK_MAX_DATA_LEN;
            let mut chunk = TxFrame::new(Seq::ZERO, chunk_cmd);
            let (p, rest) = UpdateChunkPayload::mut_from_prefix(&mut chunk.payload).unwrap();
            p.offset
                .set(u32::try_from(offset).expect("bounded by the slot capacity"));
            p.data_len
                .set(u16::try_from(data.len()).expect("bounded by the chunk size"));
            rest[..data.len()].copy_from_slice(data);
            self.send_checked(chunk, chunk_timeout)?;
            on_progress(UpdateProgress {
                sent: offset + data.len(),
                total,
            });
        }

        self.send_checked(TxFrame::new(Seq::ZERO, commit_cmd), commit_timeout)
    }

    pub fn ensure_update_supported(&mut self) -> Result<(), DriverError> {
        match first_unsupported(&self.read_cpu_version()?) {
            None => Ok(()),
            Some((device, found)) => Err(DriverError::UnsupportedFirmware {
                device,
                found,
                required: MIN_CPU_FIRMWARE_VERSION,
            }),
        }
    }

    pub fn confirm(&mut self) -> Result<(), DriverError> {
        self.send_checked(
            TxFrame::new(Seq::ZERO, Cmd::UpdateConfirm),
            UPDATE_CONFIRM_TIMEOUT,
        )
    }

    pub fn activate(&mut self) -> Result<(), DriverError> {
        self.send_checked(
            TxFrame::new(Seq::ZERO, Cmd::UpdateActivate),
            DEFAULT_TIMEOUT,
        )
    }

    pub fn close(mut self) -> Result<(), DriverError> {
        self.link.close().map_err(link_err)
    }

    #[must_use]
    pub fn into_link(self) -> L {
        self.link
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gate_names_the_first_device_below_the_minimum() {
        assert_eq!(first_unsupported(&[]), None);
        assert_eq!(first_unsupported(&[(0, 9, 0), (1, 0, 0)]), None);
        assert_eq!(
            first_unsupported(&[(0, 9, 0), (0, 8, 99), (0, 6, 1)]),
            Some((1, (0, 8, 99)))
        );
        assert_eq!(first_unsupported(&[(0, 6, 1)]), Some((0, (0, 6, 1))));
    }
}
