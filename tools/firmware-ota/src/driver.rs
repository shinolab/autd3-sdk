use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{SetModePayload, UpdateBeginPayload, UpdateChunkPayload};
use autd3_cpu_wire::{Mode, describe_device_error};
use autd3_rs_core::link::Link;
use autd3_rs_core::protocol::{Cmd, RX_FRAME_BYTES, RxFrame, Seq, TX_FRAME_BYTES, TxFrame};
use zerocopy::FromBytes;

use crate::image::CpuFirmwareImage;

pub const RESET_CYCLES: u32 = 2;
pub const DEFAULT_TIMEOUT_CYCLES: u32 = 100;
pub const UPDATE_BEGIN_TIMEOUT_CYCLES: u32 = 30_000;
pub const UPDATE_CHUNK_TIMEOUT_CYCLES: u32 = 2_000;
pub const UPDATE_COMMIT_TIMEOUT_CYCLES: u32 = 30_000;
pub const UPDATE_CONFIRM_TIMEOUT_CYCLES: u32 = 2_000;
pub const MIN_CPU_FIRMWARE_VERSION: (u8, u8, u8) = (0, 9, 0);

#[derive(Debug, thiserror::Error)]
pub enum DriverError {
    #[error("link error: {0}")]
    Link(#[source] Box<dyn core::error::Error + Send + Sync>),
    #[error("device {device} did not acknowledge {cmd:?} within {cycles} cycles")]
    Timeout {
        device: usize,
        cmd: Cmd,
        cycles: u32,
    },
    #[error("device {device} rejected {cmd:?} with firmware error {code:#04x}: {}", describe_device_error(*code))]
    Device { device: usize, cmd: Cmd, code: u8 },
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
        for _ in 0..DEFAULT_TIMEOUT_CYCLES {
            let valid = self.cycle()?;
            if valid && self.rx.iter().all(|rx| RxFrame::parse(rx).ack == Seq::ZERO) {
                self.next_seq = Seq::new(1);
                return Ok(());
            }
        }
        Err(DriverError::ModeNegotiation)
    }

    pub fn send(
        &mut self,
        mut frame: TxFrame,
        timeout_cycles: u32,
    ) -> Result<Vec<u8>, DriverError> {
        let seq = self.next_seq;
        frame.seq = seq;
        self.stage(&frame);
        let mut data = vec![None; self.num_devices()];
        for _ in 0..timeout_cycles {
            if !self.cycle()? {
                continue;
            }
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
        let device = data.iter().position(Option::is_none).unwrap_or(0);
        Err(DriverError::Timeout {
            device,
            cmd: frame.cmd,
            cycles: timeout_cycles,
        })
    }

    pub fn send_checked(&mut self, frame: TxFrame, timeout_cycles: u32) -> Result<(), DriverError> {
        let cmd = frame.cmd;
        let data = self.send(frame, timeout_cycles)?;
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
        let major = self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadCpuFwVersionMajor),
            DEFAULT_TIMEOUT_CYCLES,
        )?;
        let minor = self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadCpuFwVersionMinor),
            DEFAULT_TIMEOUT_CYCLES,
        )?;
        let patch = self.send(
            TxFrame::new(Seq::ZERO, Cmd::ReadCpuFwVersionPatch),
            DEFAULT_TIMEOUT_CYCLES,
        )?;
        Ok(major
            .into_iter()
            .zip(minor)
            .zip(patch)
            .map(|((major, minor), patch)| (major, minor, patch))
            .collect())
    }

    pub fn update(
        &mut self,
        image: &CpuFirmwareImage,
        mut on_progress: impl FnMut(UpdateProgress),
    ) -> Result<(), DriverError> {
        self.ensure_update_supported()?;
        let total = image.len();
        let mut begin = TxFrame::new(Seq::ZERO, Cmd::UpdateBegin);
        let (p, _) = UpdateBeginPayload::mut_from_prefix(&mut begin.payload).unwrap();
        p.length
            .set(u32::try_from(total).expect("bounded by the slot capacity"));
        p.crc32.set(image.crc32());
        self.send_checked(begin, UPDATE_BEGIN_TIMEOUT_CYCLES)?;
        on_progress(UpdateProgress { sent: 0, total });

        for (index, data) in image
            .as_bytes()
            .chunks(UPDATE_CHUNK_MAX_DATA_LEN)
            .enumerate()
        {
            let offset = index * UPDATE_CHUNK_MAX_DATA_LEN;
            let mut chunk = TxFrame::new(Seq::ZERO, Cmd::UpdateChunk);
            let (p, rest) = UpdateChunkPayload::mut_from_prefix(&mut chunk.payload).unwrap();
            p.offset
                .set(u32::try_from(offset).expect("bounded by the slot capacity"));
            p.data_len
                .set(u16::try_from(data.len()).expect("bounded by the chunk size"));
            rest[..data.len()].copy_from_slice(data);
            self.send_checked(chunk, UPDATE_CHUNK_TIMEOUT_CYCLES)?;
            on_progress(UpdateProgress {
                sent: offset + data.len(),
                total,
            });
        }

        self.send_checked(
            TxFrame::new(Seq::ZERO, Cmd::UpdateCommit),
            UPDATE_COMMIT_TIMEOUT_CYCLES,
        )
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
            UPDATE_CONFIRM_TIMEOUT_CYCLES,
        )
    }

    pub fn activate(&mut self) -> Result<(), DriverError> {
        self.send_checked(
            TxFrame::new(Seq::ZERO, Cmd::UpdateActivate),
            DEFAULT_TIMEOUT_CYCLES,
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
