use core::cell::Cell;
use core::num::NonZeroU32;

pub(crate) use autd3_cpu_wire::fpga_update::{
    FPGA_IMAGE_BASE, FPGA_REBOOT_DELAY_MS, FPGA_RECONFIG_SETTLE_MS, FPGA_SECTOR_BYTES,
    FpgaBootImage, is_plausible_fpga_length,
};
use autd3_cpu_wire::payload::{UpdateBeginPayload, UpdateChunkPayload};

use crate::app::Cpu;
use crate::cmd::failsafe;
use crate::fpga;
use crate::fpga_params::{
    ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1, ADDR_FLASH_CMD, ADDR_FLASH_LEN_0, ADDR_FLASH_LEN_1,
    ADDR_FLASH_RESULT_0, ADDR_FLASH_RESULT_1, ADDR_FLASH_STATUS, ADDR_FLASH_USR_ACCESS_0,
    ADDR_FLASH_USR_ACCESS_1, BramSelect, FLASH_BUF_BYTES, FlashOp, FunctionBits,
};
use crate::port::Port;
use crate::proto::{Cmd, Error};

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct FlashStatus: u16 {
        const BUSY = 1 << 0;
    }
}

const SLOT_SECTOR_BASE: u32 = FPGA_IMAGE_BASE - FPGA_IMAGE_BASE % FPGA_SECTOR_BYTES;
const FPGA_FLASH_MAX_POLLS: NonZeroU32 = NonZeroU32::new(2_000_000_000).unwrap();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Idle,
    Locked,
    Receiving {
        length: u32,
        crc32: u32,
        erased_end: u32,
    },
    Committed,
    Reboot {
        remaining_ms: u16,
    },
    Settle {
        remaining_ms: u16,
    },
}

pub(crate) struct FpgaUpdateSession {
    state: Cell<State>,
    reconfig_failed: Cell<bool>,
}

impl FpgaUpdateSession {
    pub(crate) const fn new() -> Self {
        Self {
            state: Cell::new(State::Idle),
            reconfig_failed: Cell::new(false),
        }
    }

    pub(crate) fn reconfig_failed(&self) -> bool {
        self.reconfig_failed.get()
    }

    pub(crate) fn is_locked(&self) -> bool {
        self.state.get() != State::Idle
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn state(&self) -> State {
        self.state.get()
    }
}

pub(crate) fn allowed_while_locked(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::Nop
            | Cmd::UpdateBegin
            | Cmd::UpdateChunk
            | Cmd::UpdateCommit
            | Cmd::UpdateConfirm
            | Cmd::FpgaUpdateBegin
            | Cmd::FpgaUpdateChunk
            | Cmd::FpgaUpdateCommit
            | Cmd::FpgaUpdateActivate
            | Cmd::ReadFpgaState
            | Cmd::ReadTelemetry
            | Cmd::ReadFirmwareInfo
            | Cmd::ReadRunningImage
    )
}

fn write_reg<P: Port>(port: &mut P, addr: u16, value: u16) {
    fpga::write(port, BramSelect::Flash, addr, value);
}

fn read_reg<P: Port>(port: &mut P, addr: u16) -> u16 {
    fpga::read(port, BramSelect::Flash, addr)
}

fn write_target<P: Port>(port: &mut P, addr: u32, len: u32) {
    write_reg(port, ADDR_FLASH_ADDR_0, addr as u16);
    write_reg(port, ADDR_FLASH_ADDR_1, ((addr >> 16) & 0xFF) as u16);
    write_reg(port, ADDR_FLASH_LEN_0, len as u16);
    write_reg(port, ADDR_FLASH_LEN_1, ((len >> 16) & 0xFF) as u16);
    port.memory_barrier();
}

fn target_latched<P: Port>(port: &mut P, addr: u32, len: u32) -> bool {
    read_reg(port, ADDR_FLASH_ADDR_0) == addr as u16
        && read_reg(port, ADDR_FLASH_ADDR_1) == ((addr >> 16) & 0xFF) as u16
        && read_reg(port, ADDR_FLASH_LEN_0) == len as u16
        && read_reg(port, ADDR_FLASH_LEN_1) == ((len >> 16) & 0xFF) as u16
}

fn issue<P: Port>(port: &mut P, op: FlashOp) {
    write_reg(port, ADDR_FLASH_CMD, u16::from(op.as_u8()));
    port.memory_barrier();
}

fn run_command<P: Port>(
    port: &mut P,
    max_polls: NonZeroU32,
    op: FlashOp,
    addr: u32,
    len: u32,
) -> Result<u32, Error> {
    if FlashStatus::from_bits_retain(read_reg(port, ADDR_FLASH_STATUS)).contains(FlashStatus::BUSY)
    {
        return Err(Error::FpgaTimeout);
    }
    write_target(port, addr, len);
    if !target_latched(port, addr, len) {
        return Err(Error::UpdateFlash);
    }
    issue(port, op);
    for _ in 0..max_polls.get() {
        let status = read_reg(port, ADDR_FLASH_STATUS);
        if FlashStatus::from_bits_retain(status).contains(FlashStatus::BUSY) {
            continue;
        }
        if status >> 8 != 0 {
            return Err(Error::UpdateFlash);
        }
        let lo = read_reg(port, ADDR_FLASH_RESULT_0);
        let hi = read_reg(port, ADDR_FLASH_RESULT_1);
        return Ok((u32::from(hi) << 16) | u32::from(lo));
    }
    Err(Error::FpgaTimeout)
}

fn load_buffer<P: Port>(port: &mut P, data: &[u8]) {
    for (i, word) in fpga::le_words(data, 0xFF).enumerate() {
        fpga::write(port, BramSelect::FlashBuf, i as u16, word);
    }
}

pub(crate) fn supports_flash<P: Port>(port: &mut P) -> bool {
    fpga::functions(port).contains(FunctionBits::FLASH_OTA)
}

fn boot_image<P: Port>(port: &mut P) -> FpgaBootImage {
    if !supports_flash(port) {
        return FpgaBootImage::Unknown;
    }
    let lo = read_reg(port, ADDR_FLASH_USR_ACCESS_0);
    let hi = read_reg(port, ADDR_FLASH_USR_ACCESS_1);
    FpgaBootImage::from_usr_access((u32::from(hi) << 16) | u32::from(lo))
}

impl Cpu {
    pub(crate) fn fpga_boot_image<P: Port>(&self, port: &mut P) -> FpgaBootImage {
        if self.fpga_update.reconfig_failed() {
            FpgaBootImage::ReconfigFailed
        } else {
            boot_image(port)
        }
    }

    fn fpga_reconfiguring(&self) -> bool {
        matches!(
            self.fpga_update.state.get(),
            State::Reboot { .. } | State::Settle { .. }
        )
    }

    pub(crate) fn fpga_update_begin<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        if self.fpga_reconfiguring() {
            return Err(Error::FpgaUpdateInProgress);
        }
        if self.update.is_resetting() {
            return Err(Error::UpdateActivating);
        }
        if self.fpga_update.is_locked() {
            self.fpga_update.state.set(State::Locked);
        }
        let p = UpdateBeginPayload::parse(payload)?;
        let length = p.length.get();
        if !is_plausible_fpga_length(length) {
            return Err(Error::InvalidPayload);
        }
        if !supports_flash(port) {
            return Err(Error::UpdateUnsupported);
        }
        self.fpga_update.state.set(State::Locked);
        failsafe::mute(port);
        fpga::set_watchdog_timeout(port, None);
        run_command(
            port,
            FPGA_FLASH_MAX_POLLS,
            FlashOp::Erase,
            SLOT_SECTOR_BASE,
            FPGA_SECTOR_BYTES,
        )?;
        self.fpga_update.state.set(State::Receiving {
            length,
            crc32: p.crc32.get(),
            erased_end: SLOT_SECTOR_BASE + FPGA_SECTOR_BYTES,
        });
        Ok(())
    }

    pub(crate) fn fpga_update_chunk<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let State::Receiving {
            length,
            crc32,
            erased_end,
        } = self.fpga_update.state.get()
        else {
            return Err(Error::UpdateNotStarted);
        };
        let (chunk, data) = UpdateChunkPayload::parse(payload)?;
        if !chunk.fits_in(data, length) {
            return Err(Error::InvalidPayload);
        }
        if data.is_empty() {
            return Ok(());
        }
        let start = FPGA_IMAGE_BASE + chunk.offset.get();
        let end = start + data.len() as u32;
        let max_polls = FPGA_FLASH_MAX_POLLS;
        let mut erased = erased_end;
        while erased < end {
            run_command(port, max_polls, FlashOp::Erase, erased, FPGA_SECTOR_BYTES)?;
            erased += FPGA_SECTOR_BYTES;
            self.fpga_update.state.set(State::Receiving {
                length,
                crc32,
                erased_end: erased,
            });
        }
        for (k, part) in data.chunks(FLASH_BUF_BYTES).enumerate() {
            load_buffer(port, part);
            run_command(
                port,
                max_polls,
                FlashOp::Program,
                start + (k * FLASH_BUF_BYTES) as u32,
                part.len() as u32,
            )?;
        }
        Ok(())
    }

    pub(crate) fn fpga_update_commit<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let State::Receiving { length, crc32, .. } = self.fpga_update.state.get() else {
            return Err(Error::UpdateNotStarted);
        };
        self.fpga_update.state.set(State::Locked);
        let max_polls = FPGA_FLASH_MAX_POLLS;
        if run_command(port, max_polls, FlashOp::Crc32, FPGA_IMAGE_BASE, length)? != crc32 {
            return Err(Error::UpdateImageInvalid);
        }
        self.fpga_update.state.set(State::Committed);
        Ok(())
    }

    pub(crate) fn fpga_update_activate(&self) -> Result<(), Error> {
        if self.fpga_reconfiguring() {
            return Err(Error::FpgaUpdateInProgress);
        }
        if self.fpga_update.state.get() != State::Committed {
            return Err(Error::UpdateNotCommitted);
        }
        self.fpga_update.state.set(State::Reboot {
            remaining_ms: FPGA_REBOOT_DELAY_MS,
        });
        Ok(())
    }

    pub(crate) fn fpga_update_tick<P: Port>(&self, port: &mut P) {
        match self.fpga_update.state.get() {
            State::Reboot { remaining_ms } => {
                let remaining_ms = remaining_ms.saturating_sub(1);
                if remaining_ms == 0 {
                    write_target(port, 0, 0);
                    issue(port, FlashOp::Reboot);
                    self.fpga_update.state.set(State::Settle {
                        remaining_ms: FPGA_RECONFIG_SETTLE_MS,
                    });
                } else {
                    self.fpga_update.state.set(State::Reboot { remaining_ms });
                }
            }
            State::Settle { remaining_ms } => {
                let remaining_ms = remaining_ms.saturating_sub(1);
                if remaining_ms == 0 {
                    self.finish_reconfiguration(port);
                } else {
                    self.fpga_update.state.set(State::Settle { remaining_ms });
                }
            }
            State::Idle | State::Locked | State::Receiving { .. } | State::Committed => {}
        }
    }

    fn finish_reconfiguration<P: Port>(&self, port: &mut P) {
        let reconfigured = read_reg(port, ADDR_FLASH_CMD) == 0;
        self.fpga_update.reconfig_failed.set(!reconfigured);
        if !reconfigured {
            self.fpga_update.state.set(State::Committed);
            return;
        }
        self.init(port);
        self.fpga_update.state.set(State::Idle);
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;
    use std::vec::Vec;

    use autd3_cpu_wire::fpga_update::{
        FPGA_GOLDEN_REGION_END, FPGA_IMAGE_BASE, FPGA_IMAGE_CAPACITY, FPGA_RECONFIG_WORST_MS,
        FPGA_SECTOR_BYTES, FPGA_USR_ACCESS_GOLDEN, FPGA_USR_ACCESS_UPDATE, FpgaBootImage,
    };
    use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
    use autd3_cpu_wire::update::crc32;
    use zerocopy::little_endian::U32;

    use super::{FPGA_REBOOT_DELAY_MS, FPGA_RECONFIG_SETTLE_MS, State, run_command};
    use crate::cmd::cpu_config::CpuConfig;
    use crate::cmd::update::{UpdateBeginPayload, UpdateChunkPayload};
    use crate::fpga_params::{
        ADDR_FLASH_CMD, ADDR_FLASH_LEN_0, ADDR_FLASH_STATUS, ADDR_FUNCTION_BITS, CtlFlags,
        FLASH_BUF_BYTES, FlashErr, FlashOp, FunctionBits,
    };
    use crate::proto::{Cmd, Error, OUTPUT_MASK_WORDS};
    use crate::test_utils::builders::{output_mask, set_cpu_config};
    use crate::test_utils::mock::{Frame, Harness};

    fn begin(length: u32, crc: u32) -> Frame {
        Frame::from_payload(
            Cmd::FpgaUpdateBegin,
            &UpdateBeginPayload {
                length: U32::new(length),
                crc32: U32::new(crc),
            },
        )
    }

    fn chunk(offset: u32, data: &[u8]) -> Frame {
        Frame::from_parts(
            Cmd::FpgaUpdateChunk,
            &UpdateChunkPayload {
                offset: U32::new(offset),
            },
            data,
        )
    }

    fn image(len: usize, seed: u32) -> Vec<u8> {
        let mut x = seed | 1;
        (0..len)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8
            })
            .collect()
    }

    fn ota_capable() -> Harness {
        let mut h = Harness::new();
        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::FLASH_OTA.bits()),
        );
        h
    }

    fn send_image(h: &mut Harness, img: &[u8]) {
        for (i, piece) in img.chunks(UPDATE_CHUNK_MAX_DATA_LEN).enumerate() {
            h.deliver(&chunk((i * UPDATE_CHUNK_MAX_DATA_LEN) as u32, piece));
            assert_eq!(h.status(), Error::None, "chunk {i}");
        }
    }

    fn run_update(h: &mut Harness, img: &[u8]) {
        h.deliver(&begin(img.len() as u32, crc32(img)));
        assert_eq!(h.status(), Error::None);
        send_image(h, img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::None);
    }

    fn slot(h: &Harness, len: usize) -> &[u8] {
        let base = FPGA_IMAGE_BASE as usize;
        &h.port.fpga_flash[base..][..len]
    }

    fn erased_sectors(h: &Harness) -> Vec<u32> {
        h.port
            .fpga_flash_ops
            .iter()
            .filter(|(op, _, _)| *op == FlashOp::Erase.as_u8())
            .map(|&(_, addr, len)| {
                assert_eq!(len, FPGA_SECTOR_BYTES);
                addr
            })
            .collect()
    }

    fn output_muted(h: &Harness) -> bool {
        h.ctl_flags().contains(CtlFlags::FAILSAFE)
    }

    #[test]
    fn an_fpga_without_flash_access_is_left_alone() {
        let mut h = Harness::new();
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::UpdateUnsupported);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(h.port.fpga_flash_ops, []);
        assert!(!output_muted(&h));
    }

    #[test]
    fn begin_rejects_implausible_lengths() {
        let mut h = ota_capable();
        h.deliver(&begin(0, 0));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&begin(FPGA_IMAGE_CAPACITY + 1, 0));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(h.port.fpga_flash_ops, []);
    }

    #[test]
    fn begin_mutes_the_output_and_erases_only_the_first_slot_sector() {
        let mut h = ota_capable();
        h.deliver(&begin(0x30_0000, 0));
        assert_eq!(h.status(), Error::None);
        assert!(h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));
        assert_eq!(erased_sectors(&h), [FPGA_GOLDEN_REGION_END]);
        assert!(matches!(
            h.cpu.fpga_update.state(),
            State::Receiving {
                length: 0x30_0000,
                erased_end,
                ..
            } if erased_end == FPGA_GOLDEN_REGION_END + FPGA_SECTOR_BYTES
        ));
    }

    #[test]
    fn a_full_update_writes_the_slot_and_erases_sectors_as_it_goes() {
        let mut h = ota_capable();
        let img = image(3 * FPGA_SECTOR_BYTES as usize, 7);
        run_update(&mut h, &img);
        assert_eq!(slot(&h, img.len()), &img[..]);
        assert_eq!(
            h.port.fpga_flash[FPGA_IMAGE_BASE as usize + img.len()],
            0xFF
        );
        let sectors = (FPGA_IMAGE_BASE as usize - FPGA_GOLDEN_REGION_END as usize + img.len())
            .div_ceil(FPGA_SECTOR_BYTES as usize);
        let expected: Vec<u32> = (0..sectors as u32)
            .map(|i| FPGA_GOLDEN_REGION_END + i * FPGA_SECTOR_BYTES)
            .collect();
        assert_eq!(erased_sectors(&h), expected);
        assert!(
            h.port
                .fpga_flash_ops
                .iter()
                .filter(|(op, _, _)| *op == FlashOp::Program.as_u8())
                .all(|&(_, addr, len)| addr >= FPGA_IMAGE_BASE && len as usize <= FLASH_BUF_BYTES)
        );
        assert_eq!(
            h.port.fpga_flash_ops.last(),
            Some(&(FlashOp::Crc32.as_u8(), FPGA_IMAGE_BASE, img.len() as u32))
        );
        assert_eq!(h.cpu.fpga_update.state(), State::Committed);
        let erased_sector = vec![0xFF; FPGA_SECTOR_BYTES as usize];
        assert!(
            h.port.fpga_flash[..FPGA_GOLDEN_REGION_END as usize]
                .chunks(FPGA_SECTOR_BYTES as usize)
                .all(|sector| sector == erased_sector)
        );
    }

    #[test]
    fn a_chunk_longer_than_the_flash_buffer_is_programmed_in_parts() {
        let mut h = ota_capable();
        let img = image(UPDATE_CHUNK_MAX_DATA_LEN, 11);
        h.deliver(&begin(img.len() as u32, crc32(&img)));
        h.port.fpga_flash_ops.clear();
        h.deliver(&chunk(0, &img));
        assert_eq!(h.status(), Error::None);
        let programs: Vec<(u32, u32)> = h
            .port
            .fpga_flash_ops
            .iter()
            .filter(|(op, _, _)| *op == FlashOp::Program.as_u8())
            .map(|&(_, addr, len)| (addr, len))
            .collect();
        let split = FLASH_BUF_BYTES as u32;
        assert_eq!(
            programs,
            [
                (FPGA_IMAGE_BASE, split),
                (FPGA_IMAGE_BASE + split, img.len() as u32 - split),
            ]
        );
        assert_eq!(slot(&h, img.len()), &img[..]);
    }

    #[test]
    fn a_retransmitted_chunk_is_harmless() {
        let mut h = ota_capable();
        let img = image(2000, 3);
        h.deliver(&begin(img.len() as u32, crc32(&img)));
        h.deliver(&chunk(0, &img[..600]));
        assert_eq!(h.status(), Error::None);
        let erases = erased_sectors(&h).len();
        h.deliver(&chunk(0, &img[..600]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(erased_sectors(&h).len(), erases);
        h.deliver(&chunk(600, &img[600..1200]));
        h.deliver(&chunk(1200, &img[1200..1800]));
        h.deliver(&chunk(1800, &img[1800..]));
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn chunks_are_validated() {
        let mut h = ota_capable();
        h.deliver(&chunk(0, &[1, 2, 3]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&begin(10, 0));
        h.deliver(&chunk(8, &[1, 2, 3]));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&chunk(11, &[]));
        assert_eq!(h.status(), Error::InvalidPayload);
        let before = h.port.fpga_flash_ops.len();
        h.deliver(&chunk(10, &[]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.fpga_flash_ops.len(), before);
        h.deliver(&chunk(u32::MAX - 1, &[1, 2, 3]));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.port.fpga_flash_ops.len(), before);
    }

    #[test]
    fn an_odd_sized_chunk_pads_the_last_word_with_erased_bits() {
        let mut h = ota_capable();
        let img = image(5, 9);
        run_update(&mut h, &img);
        assert_eq!(slot(&h, 6), &[img[0], img[1], img[2], img[3], img[4], 0xFF]);
    }

    #[test]
    fn a_crc_mismatch_keeps_the_device_locked_and_needs_a_new_session() {
        let mut h = ota_capable();
        let img = image(1000, 5);
        h.deliver(&begin(img.len() as u32, crc32(&img) ^ 1));
        send_image(&mut h, &img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::UpdateImageInvalid);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        assert!(h.cpu.fpga_update.is_locked());
        h.deliver(&chunk(0, &img[..10]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::UpdateNotCommitted);
    }

    #[test]
    fn a_new_session_after_a_crc_mismatch_completes_without_a_power_cycle() {
        let mut h = ota_capable();
        let img = image(1000, 6);
        h.deliver(&begin(img.len() as u32, crc32(&img) ^ 1));
        send_image(&mut h, &img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::UpdateImageInvalid);

        let retry = image(1000, 7);
        run_update(&mut h, &retry);
        assert_eq!(h.cpu.fpga_update.state(), State::Committed);
        assert_eq!(slot(&h, retry.len()), retry.as_slice());
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert_eq!(h.port.fpga_reboots, 1);
        assert!(!h.cpu.fpga_update.is_locked());
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn commit_without_a_session_is_rejected() {
        let mut h = ota_capable();
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::UpdateNotCommitted);
    }

    #[test]
    fn the_failsafe_cannot_be_released_while_locked() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        assert!(output_muted(&h));
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        assert!(output_muted(&h));
    }

    #[test]
    fn raw_fpga_memory_access_is_rejected_while_locked() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        for cmd in [Cmd::WriteFpgaMemory, Cmd::ReadFpgaMemory] {
            h.deliver(&Frame::new(cmd));
            assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        }
        assert!(output_muted(&h));
    }

    #[test]
    fn output_commands_are_rejected_while_locked() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        let before = h.port.output_mask.clone();
        h.deliver(&output_mask(&[true; 249]));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        assert_eq!(h.port.output_mask, before);
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        assert_eq!(h.port.output_mask, before);
        h.deliver(&Frame::new(Cmd::Synchronize));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        h.deliver(&Frame::new(Cmd::Nop));
        assert_eq!(h.status(), Error::None);
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(h.status(), Error::None);
        h.deliver(&Frame::new(Cmd::Reboot));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        assert_eq!(h.port.reset_count, 0);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn a_rejected_begin_does_not_release_the_lock() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::None);
        h.deliver(&begin(0, 0));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        h.set_ctl(ADDR_FUNCTION_BITS, 0);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::UpdateUnsupported);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        assert!(output_muted(&h));
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
    }

    #[test]
    fn a_rejected_begin_discards_a_committed_image() {
        let mut h = ota_capable();
        run_update(&mut h, &image(700, 13));
        h.deliver(&begin(0, 0));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::UpdateNotCommitted);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS));
        assert_eq!(h.port.fpga_reboots, 0);
    }

    #[test]
    fn a_floating_bus_is_not_mistaken_for_flash_support() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FUNCTION_BITS, 0xFFFF);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::UpdateUnsupported);
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().fpga_boot_image,
            FpgaBootImage::Unknown as u8
        );
    }

    #[test]
    fn activation_reboots_the_fpga_and_reinitializes_it_later() {
        let mut h = ota_capable();
        let img = image(700, 11);
        run_update(&mut h, &img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.fpga_reboots, 0);

        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) - 1);
        assert_eq!(h.port.fpga_reboots, 0);
        h.tick_1ms(1);
        assert_eq!(h.port.fpga_reboots, 1);
        assert_eq!(
            h.port.fpga_flash_ops.last().map(|op| op.0),
            Some(FlashOp::Reboot.as_u8())
        );
        assert_eq!(
            h.cpu.fpga_update.state(),
            State::Settle {
                remaining_ms: FPGA_RECONFIG_SETTLE_MS,
            }
        );

        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        h.deliver(&chunk(0, &img[..10]));
        assert_eq!(h.status(), Error::UpdateNotStarted);
        h.deliver(&Frame::new(Cmd::FpgaUpdateCommit));
        assert_eq!(h.status(), Error::UpdateNotStarted);

        h.tick_1ms(u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
        assert!(h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));
        h.tick_1ms(1);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(h.cpu.fpga_update.state(), State::Idle);
        assert!((0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0xFFFF));
        h.tick_1ms(10_000);
        assert_eq!(h.port.fpga_reboots, 1);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
    }

    fn activated(seed: u32) -> Harness {
        let mut h = ota_capable();
        let img = image(700, seed);
        run_update(&mut h, &img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h
    }

    fn reboot_requests(h: &Harness) -> usize {
        h.port
            .fpga_flash_ops
            .iter()
            .filter(|(op, _, _)| *op == FlashOp::Reboot.as_u8())
            .count()
    }

    fn boot_image(h: &mut Harness) -> u8 {
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        h.firmware_info().fpga_boot_image
    }

    #[test]
    fn an_ignored_reboot_leaves_the_image_committed_for_another_activation() {
        let mut h = activated(43);
        h.port.fpga_reboots_to_ignore = 1;
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert_eq!(reboot_requests(&h), 1);
        assert_eq!(h.port.fpga_reboots, 0);
        assert_eq!(h.cpu.fpga_update.state(), State::Committed);
        assert!(h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));
        assert_eq!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);
        h.tick_1ms(10_000);
        assert_eq!(reboot_requests(&h), 1);

        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert_eq!(reboot_requests(&h), 2);
        assert_eq!(h.port.fpga_reboots, 1);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(h.cpu.fpga_update.state(), State::Idle);
        assert!((0..OUTPUT_MASK_WORDS).all(|i| h.output_mask(i) == 0xFFFF));
        assert_ne!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);
    }

    #[test]
    fn a_reinitialization_that_times_out_after_reconfiguration_counts_a_boot_failure() {
        let mut h = activated(47);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
        assert_eq!(h.telemetry().boot_failure, 0);
        h.port.latch_stuck = true;
        h.tick_1ms(1);
        h.port.latch_stuck = false;
        assert_eq!(h.telemetry().boot_failure, 1);
        assert!(!h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert!(!output_muted(&h));
        assert_eq!(h.telemetry().boot_failure, 1);
    }

    #[test]
    fn the_watchdog_is_suspended_from_begin_until_the_reinitialization() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.watchdog_timeout, Some(0));
        let mut h = activated(61);
        assert_eq!(h.port.watchdog_timeout, Some(0));
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
        assert_eq!(h.port.watchdog_timeout, Some(0));
        h.tick_1ms(1);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(
            h.port.watchdog_timeout,
            Some(autd3_cpu_wire::cpu_params::CPU_WATCHDOG_TIMEOUT.as_millis() as u16)
        );
    }

    #[test]
    fn a_clean_reinitialization_after_reconfiguration_counts_no_boot_failure() {
        let mut h = activated(48);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert!(!h.cpu.fpga_update.is_locked());
        assert!(!output_muted(&h));
        assert_eq!(h.telemetry().boot_failure, 0);
    }

    fn ptp_failsafe_then_activated(seed: u32) -> Harness {
        let mut h = ota_capable();
        let config = h.cpu.config();
        h.deliver(&set_cpu_config(&CpuConfig {
            ptp_unlock_failsafe_timeout: Some(core::time::Duration::from_millis(50)),
            ..config
        }));
        assert_eq!(h.status(), Error::None);
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert!(h.cpu.ptp_failsafe_active());
        h.cpu.publish_ptp_unlocked(None);
        run_update(&mut h, &image(700, seed));
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h
    }

    #[test]
    fn a_clean_reinitialization_after_reconfiguration_lowers_the_ptp_failsafe_flag() {
        let mut h = ptp_failsafe_then_activated(63);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
        assert!(h.cpu.ptp_failsafe_active());
        h.tick_1ms(1);
        assert!(!output_muted(&h));
        assert!(!h.cpu.ptp_failsafe_active());
    }

    #[test]
    fn a_reinitialization_that_times_out_keeps_the_ptp_failsafe_flag() {
        let mut h = ptp_failsafe_then_activated(64);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS) - 1);
        h.port.latch_stuck = true;
        h.tick_1ms(1);
        h.port.latch_stuck = false;
        assert!(output_muted(&h));
        assert!(h.cpu.ptp_failsafe_active());
    }

    #[test]
    fn an_fpga_that_never_reconfigures_is_reported_and_stays_locked() {
        let mut h = activated(47);
        h.port.fpga_reboots_to_ignore = u32::MAX;
        h.tick_1ms(FPGA_RECONFIG_WORST_MS - 1);
        assert_eq!(reboot_requests(&h), 1);
        assert!(matches!(h.cpu.fpga_update.state(), State::Settle { .. }));
        h.tick_1ms(1);
        assert_eq!(h.cpu.fpga_update.state(), State::Committed);
        assert!(h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));
        assert_eq!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
        h.tick_1ms(10_000);
        assert_eq!(reboot_requests(&h), 1);
        assert_eq!(h.port.fpga_reboots, 0);
    }

    #[test]
    fn a_later_successful_reconfiguration_clears_the_failure() {
        let mut h = activated(59);
        h.port.fpga_reboots_to_ignore = u32::MAX;
        h.tick_1ms(FPGA_RECONFIG_WORST_MS);
        assert_eq!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);

        h.port.fpga_reboots_to_ignore = 0;
        run_update(&mut h, &image(700, 61));
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert!(!h.cpu.fpga_update.is_locked());
        assert_ne!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);
    }

    #[test]
    fn a_reboot_write_ignored_while_busy_is_detected() {
        let mut h = activated(53);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) - 1);
        h.port.fpga_flash_dropped_reg = Some(ADDR_FLASH_CMD);
        h.tick_1ms(1 + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert_eq!(h.port.fpga_reboots, 0);
        assert_eq!(h.cpu.fpga_update.state(), State::Committed);
        assert_eq!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);

        h.port.fpga_flash_dropped_reg = None;
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        assert_eq!(h.status(), Error::None);
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS) + u32::from(FPGA_RECONFIG_SETTLE_MS));
        assert_eq!(h.port.fpga_reboots, 1);
        assert!(!h.cpu.fpga_update.is_locked());
        assert_ne!(boot_image(&mut h), FpgaBootImage::ReconfigFailed as u8);
    }

    #[test]
    fn flash_errors_surface_as_update_flash() {
        let mut h = ota_capable();
        h.port.fpga_flash_err = Some(FlashErr::Protected);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        assert!(h.cpu.fpga_update.is_locked());
        assert!(output_muted(&h));
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::FpgaUpdateInProgress);
    }

    #[test]
    fn an_erase_failure_mid_chunk_keeps_the_session_and_the_erased_range() {
        let mut h = ota_capable();
        let img = image(FPGA_SECTOR_BYTES as usize + 100, 36);
        h.deliver(&begin(img.len() as u32, crc32(&img)));
        assert_eq!(h.status(), Error::None);
        let State::Receiving { erased_end, .. } = h.cpu.fpga_update.state() else {
            panic!("session did not open");
        };
        h.port.fpga_flash_err = Some(FlashErr::Protected);
        h.deliver(&chunk(
            FPGA_SECTOR_BYTES,
            &img[FPGA_SECTOR_BYTES as usize..],
        ));
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(erased_sectors(&h).len(), 2);
        assert_eq!(
            h.cpu.fpga_update.state(),
            State::Receiving {
                length: img.len() as u32,
                crc32: crc32(&img),
                erased_end,
            }
        );
        assert!(h.cpu.fpga_update.is_locked());
    }

    #[test]
    fn every_flash_command_is_issued_after_its_target_is_flushed() {
        let mut h = ota_capable();
        let img = image(3 * FPGA_SECTOR_BYTES as usize, 41);
        run_update(&mut h, &img);
        h.deliver(&Frame::new(Cmd::FpgaUpdateActivate));
        h.tick_1ms(u32::from(FPGA_REBOOT_DELAY_MS));
        assert_eq!(h.port.fpga_reboots, 1);
        assert!(h.port.fpga_flash_ops.len() > 3);
        assert_eq!(h.port.fpga_flash_cmds_before_flush, 0);
    }

    #[test]
    fn a_lost_target_write_stops_the_command() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::None);
        let ops = h.port.fpga_flash_ops.len();
        h.port.fpga_flash_dropped_reg = Some(ADDR_FLASH_LEN_0);
        h.deliver(&chunk(0, &[0xAB; 16]));
        assert_eq!(h.status(), Error::UpdateFlash);
        assert_eq!(h.port.fpga_flash_ops.len(), ops);
    }

    #[test]
    fn a_busy_fpga_times_out_without_issuing_a_command() {
        let mut h = ota_capable();
        h.port.fpga_flash_hang = true;
        h.port.fpga_flash_reg[usize::from(ADDR_FLASH_STATUS)] = 1;
        let ops = h.port.fpga_flash_ops.len();
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::FpgaTimeout);
        assert_eq!(h.cpu.fpga_update.state(), State::Locked);
        assert_eq!(h.port.fpga_flash_ops.len(), ops);
        h.deliver(&begin(100, 0));
        assert_eq!(h.status(), Error::FpgaTimeout);
        assert_eq!(h.port.fpga_flash_ops.len(), ops);
    }

    #[test]
    fn a_hung_fpga_is_polled_the_given_number_of_times() {
        let mut h = ota_capable();
        h.port.fpga_flash_hang = true;
        let reads = h.port.fpga_flash_status_reads;
        assert_eq!(
            run_command(
                &mut h.port,
                core::num::NonZeroU32::new(7).unwrap(),
                FlashOp::Erase,
                FPGA_IMAGE_BASE,
                FPGA_SECTOR_BYTES,
            ),
            Err(Error::FpgaTimeout)
        );
        assert_eq!(h.port.fpga_flash_status_reads - reads, 1 + 7);
    }

    #[test]
    fn a_chunk_erases_every_sector_it_reaches() {
        let mut h = ota_capable();
        let length = 3 * FPGA_SECTOR_BYTES;
        h.deliver(&begin(length, 0));
        let far = 2 * FPGA_SECTOR_BYTES + 10;
        h.deliver(&chunk(far, &[0xAB; 16]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            erased_sectors(&h),
            [
                FPGA_GOLDEN_REGION_END,
                FPGA_GOLDEN_REGION_END + FPGA_SECTOR_BYTES,
                FPGA_GOLDEN_REGION_END + 2 * FPGA_SECTOR_BYTES
            ]
        );
        h.deliver(&chunk(0, &[0xCD; 16]));
        assert_eq!(erased_sectors(&h).len(), 3);
    }

    #[test]
    fn boot_image_reports_the_usr_access_value() {
        let mut h = Harness::new();
        h.port.fpga_usr_access = FPGA_USR_ACCESS_UPDATE;
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().fpga_boot_image,
            FpgaBootImage::Unknown as u8
        );

        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::FLASH_OTA.bits()),
        );
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().fpga_boot_image,
            FpgaBootImage::Update as u8
        );
        h.port.fpga_usr_access = FPGA_USR_ACCESS_GOLDEN;
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().fpga_boot_image,
            FpgaBootImage::Golden as u8
        );
        h.port.fpga_usr_access = 0xFFFF_FFFF;
        h.deliver(&Frame::new(Cmd::ReadFirmwareInfo));
        assert_eq!(
            h.firmware_info().fpga_boot_image,
            FpgaBootImage::Unknown as u8
        );
    }

    #[test]
    fn a_cpu_boot_clears_the_fpga_session() {
        let mut h = ota_capable();
        h.deliver(&begin(100, 0));
        assert!(h.cpu.fpga_update.is_locked());
        h.reboot();
        assert!(!h.cpu.fpga_update.is_locked());
        assert_eq!(h.cpu.fpga_update.state(), State::Idle);
    }
}
