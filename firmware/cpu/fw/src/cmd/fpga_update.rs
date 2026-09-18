use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};

use zerocopy::FromBytes;

pub use autd3_cpu_wire::fpga_update::{
    FPGA_FUNC_FLASH_OTA, FPGA_IMAGE_BASE, FPGA_REBOOT_ATTEMPTS, FPGA_REBOOT_DELAY_MS,
    FPGA_RECONFIG_SETTLE_MS, FPGA_SECTOR_BYTES, FpgaBootImage, is_plausible_fpga_length,
};
use autd3_cpu_wire::layout::UPDATE_CHUNK_MAX_DATA_LEN;
use autd3_cpu_wire::payload::{UpdateBeginPayload, UpdateChunkPayload};

use crate::app::Cpu;
use crate::cmd::failsafe;
use crate::fpga;
use crate::params::{
    ADDR_FLASH_ADDR_0, ADDR_FLASH_ADDR_1, ADDR_FLASH_CMD, ADDR_FLASH_LEN_0, ADDR_FLASH_LEN_1,
    ADDR_FLASH_RESULT_0, ADDR_FLASH_RESULT_1, ADDR_FLASH_STATUS, ADDR_FLASH_USR_ACCESS_0,
    ADDR_FLASH_USR_ACCESS_1, ADDR_VERSION_NUM_MAJOR, BRAM_CNT_SELECT_FLASH,
    BRAM_CNT_SELECT_FLASH_BUF, BRAM_SELECT_CONTROLLER, FLASH_BUF_BYTES, FLASH_OP_CRC32,
    FLASH_OP_ERASE, FLASH_OP_PROGRAM, FLASH_OP_REBOOT,
};
use crate::port::Port;
use crate::proto::{Cmd, Error};

#[cfg(not(test))]
pub(crate) const FPGA_FLASH_MAX_POLLS: u32 = 2_000_000_000;
#[cfg(test)]
pub(crate) const FPGA_FLASH_MAX_POLLS: u32 = 10_000;

const FLASH_STATUS_BUSY: u16 = 1 << 0;
const SLOT_SECTOR_BASE: u32 = FPGA_IMAGE_BASE - FPGA_IMAGE_BASE % FPGA_SECTOR_BYTES;

const _: () = assert!(UPDATE_CHUNK_MAX_DATA_LEN <= FLASH_BUF_BYTES);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum State {
    Idle,
    Receiving {
        length: u32,
        crc32: u32,
        erased_end: u32,
    },
    Committed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Reconfig {
    None,
    Reboot(u16),
    Settle(u16),
}

pub(crate) struct FpgaUpdateSession {
    state: Cell<State>,
    reconfig: Cell<Reconfig>,
    reboot_attempts: Cell<u8>,
    locked: AtomicBool,
}

impl FpgaUpdateSession {
    pub(crate) const fn new() -> Self {
        Self {
            state: Cell::new(State::Idle),
            reconfig: Cell::new(Reconfig::None),
            reboot_attempts: Cell::new(0),
            locked: AtomicBool::new(false),
        }
    }

    pub(crate) fn init(&self) {
        self.state.set(State::Idle);
        self.reconfig.set(Reconfig::None);
        self.reboot_attempts.set(0);
        self.locked.store(false, Ordering::Relaxed);
    }

    pub(crate) fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn state(&self) -> State {
        self.state.get()
    }

    #[cfg(all(test, not(loom)))]
    pub(crate) fn reconfig(&self) -> Reconfig {
        self.reconfig.get()
    }
}

pub(crate) fn is_fpga_update_cmd(cmd: Cmd) -> bool {
    matches!(
        cmd,
        Cmd::FpgaUpdateBegin
            | Cmd::FpgaUpdateChunk
            | Cmd::FpgaUpdateCommit
            | Cmd::FpgaUpdateActivate
    )
}

pub(crate) fn allowed_while_locked(cmd: Cmd) -> bool {
    cmd != Cmd::UpdateActivate
        && (matches!(cmd, Cmd::Reset | Cmd::Nop | Cmd::SetMode)
            || matches!(cmd.as_u8() >> 4, 0x7 | 0xE))
}

fn flash_reg(addr: u16) -> u16 {
    (u16::from(BRAM_CNT_SELECT_FLASH) << 8) | addr
}

fn flash_buf(index: usize) -> u16 {
    (u16::from(BRAM_CNT_SELECT_FLASH_BUF) << 8) + index as u16
}

fn write_reg<P: Port>(port: &mut P, addr: u16, value: u16) {
    fpga::write(port, BRAM_SELECT_CONTROLLER, flash_reg(addr), value);
}

fn read_reg<P: Port>(port: &mut P, addr: u16) -> u16 {
    fpga::read(port, BRAM_SELECT_CONTROLLER, flash_reg(addr))
}

const TARGET_REGS: [u16; 4] = [
    ADDR_FLASH_ADDR_0,
    ADDR_FLASH_ADDR_1,
    ADDR_FLASH_LEN_0,
    ADDR_FLASH_LEN_1,
];

fn target_words(addr: u32, len: u32) -> [u16; 4] {
    [
        addr as u16,
        ((addr >> 16) & 0xFF) as u16,
        len as u16,
        ((len >> 16) & 0xFF) as u16,
    ]
}

fn write_target<P: Port>(port: &mut P, addr: u32, len: u32) {
    for (reg, word) in TARGET_REGS.into_iter().zip(target_words(addr, len)) {
        write_reg(port, reg, word);
    }
    port.memory_barrier();
}

fn target_latched<P: Port>(port: &mut P, addr: u32, len: u32) -> bool {
    TARGET_REGS
        .into_iter()
        .zip(target_words(addr, len))
        .all(|(reg, word)| read_reg(port, reg) == word)
}

fn issue<P: Port>(port: &mut P, op: u8) {
    write_reg(port, ADDR_FLASH_CMD, u16::from(op));
    port.memory_barrier();
}

fn start_command<P: Port>(port: &mut P, op: u8, addr: u32, len: u32) {
    write_target(port, addr, len);
    issue(port, op);
}

fn run_command<P: Port>(port: &mut P, op: u8, addr: u32, len: u32) -> Result<u32, Error> {
    if read_reg(port, ADDR_FLASH_STATUS) & FLASH_STATUS_BUSY != 0 {
        return Err(Error::FpgaTimeout);
    }
    write_target(port, addr, len);
    if !target_latched(port, addr, len) {
        return Err(Error::UpdateFlash);
    }
    issue(port, op);
    for _ in 0..FPGA_FLASH_MAX_POLLS {
        let status = read_reg(port, ADDR_FLASH_STATUS);
        if status & FLASH_STATUS_BUSY != 0 {
            continue;
        }
        if status >> 8 != 0 {
            return Err(Error::UpdateFlash);
        }
        let lo = read_reg(port, ADDR_FLASH_RESULT_0);
        let hi = read_reg(port, ADDR_FLASH_RESULT_1);
        return Ok(u32::from(lo) | (u32::from(hi) << 16));
    }
    Err(Error::FpgaTimeout)
}

fn load_buffer<P: Port>(port: &mut P, data: &[u8]) {
    for (i, pair) in data.chunks(2).enumerate() {
        let lo = u16::from(pair[0]);
        let hi = u16::from(pair.get(1).copied().unwrap_or(0xFF));
        fpga::write(port, BRAM_SELECT_CONTROLLER, flash_buf(i), lo | (hi << 8));
    }
}

pub(crate) fn supports_flash<P: Port>(port: &mut P) -> bool {
    let functions = (fpga::read(port, BRAM_SELECT_CONTROLLER, ADDR_VERSION_NUM_MAJOR) >> 8) as u8;
    functions != 0xFF && functions & FPGA_FUNC_FLASH_OTA != 0
}

pub(crate) fn boot_image<P: Port>(port: &mut P) -> FpgaBootImage {
    if !supports_flash(port) {
        return FpgaBootImage::Unknown;
    }
    let lo = read_reg(port, ADDR_FLASH_USR_ACCESS_0);
    let hi = read_reg(port, ADDR_FLASH_USR_ACCESS_1);
    FpgaBootImage::from_usr_access(u32::from(lo) | (u32::from(hi) << 16))
}

impl Cpu {
    fn fpga_reconfiguring(&self) -> bool {
        self.fpga_update.reconfig.get() != Reconfig::None
    }

    pub(crate) fn fpga_update_begin<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        if self.fpga_reconfiguring() {
            return Err(Error::FpgaUpdateInProgress);
        }
        if self.update.is_activating() {
            return Err(Error::UpdateActivating);
        }
        self.fpga_update.state.set(State::Idle);
        let Ok((p, _)) = UpdateBeginPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let length = p.length.get();
        if !is_plausible_fpga_length(length) {
            return Err(Error::InvalidPayload);
        }
        if !supports_flash(port) {
            return Err(Error::UpdateUnsupported);
        }
        self.fpga_update.locked.store(true, Ordering::Relaxed);
        failsafe::mute(port);
        run_command(port, FLASH_OP_ERASE, SLOT_SECTOR_BASE, FPGA_SECTOR_BYTES)?;
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
        let Ok((p, rest)) = UpdateChunkPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let offset = p.offset.get();
        let data_len = p.data_len.get();
        if usize::from(data_len) > UPDATE_CHUNK_MAX_DATA_LEN
            || offset > length
            || u32::from(data_len) > length - offset
        {
            return Err(Error::InvalidPayload);
        }
        if data_len == 0 {
            return Ok(());
        }
        let start = FPGA_IMAGE_BASE + offset;
        let end = start + u32::from(data_len);
        let mut erased = erased_end;
        while erased < end {
            run_command(port, FLASH_OP_ERASE, erased, FPGA_SECTOR_BYTES)?;
            erased += FPGA_SECTOR_BYTES;
            self.fpga_update.state.set(State::Receiving {
                length,
                crc32,
                erased_end: erased,
            });
        }
        load_buffer(port, &rest[..usize::from(data_len)]);
        run_command(port, FLASH_OP_PROGRAM, start, u32::from(data_len))?;
        Ok(())
    }

    pub(crate) fn fpga_update_commit<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        let State::Receiving { length, crc32, .. } = self.fpga_update.state.get() else {
            return Err(Error::UpdateNotStarted);
        };
        self.fpga_update.state.set(State::Idle);
        if run_command(port, FLASH_OP_CRC32, FPGA_IMAGE_BASE, length)? != crc32 {
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
        self.fpga_update.reboot_attempts.set(0);
        self.fpga_update
            .reconfig
            .set(Reconfig::Reboot(FPGA_REBOOT_DELAY_MS));
        Ok(())
    }

    pub(crate) fn fpga_update_tick<P: Port>(&self, port: &mut P) {
        match self.fpga_update.reconfig.get() {
            Reconfig::None => {}
            Reconfig::Reboot(remaining) => {
                let remaining = remaining.saturating_sub(1);
                if remaining == 0 {
                    let attempts = &self.fpga_update.reboot_attempts;
                    attempts.set(attempts.get() + 1);
                    start_command(port, FLASH_OP_REBOOT, 0, 0);
                    self.fpga_update
                        .reconfig
                        .set(Reconfig::Settle(FPGA_RECONFIG_SETTLE_MS));
                } else {
                    self.fpga_update.reconfig.set(Reconfig::Reboot(remaining));
                }
            }
            Reconfig::Settle(remaining) => {
                let remaining = remaining.saturating_sub(1);
                if remaining == 0 {
                    self.finish_reconfiguration(port);
                } else {
                    self.fpga_update.reconfig.set(Reconfig::Settle(remaining));
                }
            }
        }
    }

    fn finish_reconfiguration<P: Port>(&self, port: &mut P) {
        let reconfigured = read_reg(port, ADDR_FLASH_CMD) == 0;
        if !reconfigured && self.fpga_update.reboot_attempts.get() < FPGA_REBOOT_ATTEMPTS {
            self.fpga_update.reconfig.set(Reconfig::Reboot(1));
            return;
        }
        if !reconfigured {
            self.record_error_detail(Error::FpgaReconfigFailed);
        }
        self.fpga_update.state.set(State::Idle);
        self.fpga_update.reconfig.set(Reconfig::None);
        self.reinit_fpga(port);
        self.fpga_update.locked.store(false, Ordering::Relaxed);
    }
}
