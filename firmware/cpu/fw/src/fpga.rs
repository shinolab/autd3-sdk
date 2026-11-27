use core::num::NonZeroU32;
use core::time::Duration;

use autd3_cpu_wire::config::FpgaConfig;
use autd3_cpu_wire::cpu_params::PWE_DEFAULT_TABLE;
use autd3_cpu_wire::payload::{SyncPsStats, TDC_CHUNK_SAMPLES, TdcChunk, TdcSource};
use autd3_cpu_wire::value::SysTime;
use zerocopy::little_endian::{I16, U16};

use crate::fpga_params::{
    ADDR_CTL_FLAG, ADDR_DEBUG_VALUE0_0, ADDR_FUNCTION_BITS, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0,
    ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0, ADDR_PATTERN_CYCLE0,
    ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, ADDR_PATTERN_MODE0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
    ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG, ADDR_SILENCER_SET_RESULT,
    ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE, ADDR_SYNC_PS_CTL,
    ADDR_SYNC_PS_DIFF_MAX, ADDR_SYNC_PS_DIFF_MIN, ADDR_SYNC_PS_DIFF_OUTLIERS,
    ADDR_SYNC_PS_FINE_EXITS, ADDR_SYNC_PS_KF, ADDR_SYNC_PS_KI, ADDR_SYNC_PS_KP,
    ADDR_SYNC_PS_RATE_MAX, ADDR_SYNC_PS_RATE_MIN, ADDR_SYNC_PS_STATS_CTL, ADDR_SYNC_PS_TDC_BUF,
    ADDR_WATCHDOG_KICK, ADDR_WATCHDOG_RELEASE, ADDR_WATCHDOG_STATE, ADDR_WATCHDOG_TIMEOUT,
    BramSelect, CtlFlags, FunctionBits, NUM_BANKS, NUM_TRANSDUCERS,
    SET_RESULT_TRANSITION_INVALID_BIT,
};
pub use crate::fpga_params::{PWE_TABLE_SIZE, REP_INFINITE};
use crate::port::Port;
use crate::proto::{Error, OUTPUT_MASK_WORDS};

pub const FPGA_WAIT_UPDATE_MAX_POLLS: NonZeroU32 = NonZeroU32::new(1_000_000).unwrap();
pub const FPGA_PAGE_WORDS: u32 = 16384;

pub use autd3_cpu_wire::payload::{EmissionType, TransitionModeKind};

#[must_use]
pub fn transition_register_value(
    transition_mode: TransitionModeKind,
    transition_value: u64,
) -> u64 {
    if transition_mode == TransitionModeKind::SysTime {
        SysTime::from_nanos(transition_value).fpga_ticks()
    } else {
        transition_value
    }
}

pub use autd3_cpu_wire::payload::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE,
};
pub const PHASE_CORR_WORDS: usize = NUM_TRANSDUCERS.div_ceil(2);
const DEBUG_VALUE_WORDS: u16 = 16;

fn le_words(data: &[u8], pad: u8) -> impl Iterator<Item = u16> {
    let (pairs, tail) = data.as_chunks::<2>();
    pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .chain(tail.first().map(|byte| u16::from_le_bytes([*byte, pad])))
}

pub struct Bram<'a, P: Port> {
    port: &'a mut P,
    select: BramSelect,
}

impl<P: Port> Bram<'_, P> {
    fn addr(select: BramSelect, addr: u16) -> u16 {
        (u16::from(select.as_u8()) << 8) | addr
    }

    pub fn write(&mut self, addr: u16, value: u16) {
        self.port.fpga_write(Self::addr(self.select, addr), value);
    }

    #[must_use]
    pub fn read(&mut self, addr: u16) -> u16 {
        self.port.fpga_read(Self::addr(self.select, addr))
    }

    pub fn write_words(&mut self, words: impl IntoIterator<Item = u16>) {
        (0..)
            .zip(words)
            .for_each(|(addr, word)| self.write(addr, word));
    }

    pub fn write_le_bytes(&mut self, src: &[u8], pad: u8) {
        self.write_words(le_words(src, pad));
    }
}

pub fn phase_corr<P: Port>(port: &mut P) -> Bram<'_, P> {
    Bram {
        port,
        select: BramSelect::PhaseCorr,
    }
}

pub fn output_mask<P: Port>(port: &mut P) -> Bram<'_, P> {
    Bram {
        port,
        select: BramSelect::OutputMask,
    }
}

pub fn pwe_table<P: Port>(port: &mut P) -> Bram<'_, P> {
    Bram {
        port,
        select: BramSelect::PweTable,
    }
}

pub fn flash<P: Port>(port: &mut P) -> Bram<'_, P> {
    Bram {
        port,
        select: BramSelect::Flash,
    }
}

pub fn flash_buf<P: Port>(port: &mut P) -> Bram<'_, P> {
    Bram {
        port,
        select: BramSelect::FlashBuf,
    }
}

pub struct Controller<'a, P: Port> {
    bram: Bram<'a, P>,
}

pub fn controller<P: Port>(port: &mut P) -> Controller<'_, P> {
    Controller {
        bram: Bram {
            select: BramSelect::Controller,
            port,
        },
    }
}

impl<P: Port> Controller<'_, P> {
    pub fn write(&mut self, addr: u16, value: u16) {
        self.bram.write(addr, value);
    }

    #[must_use]
    pub fn read(&mut self, addr: u16) -> u16 {
        self.bram.read(addr)
    }

    pub fn write_u64(&mut self, addr: u16, value: u64) {
        for i in 0..4u16 {
            self.write(addr + i, (value >> (16 * i)) as u16);
        }
    }

    #[must_use]
    pub fn functions(&mut self) -> FunctionBits {
        match self.read(ADDR_FUNCTION_BITS) as u8 {
            0xFF => FunctionBits::empty(),
            bits => FunctionBits::from_bits_retain(bits),
        }
    }

    pub fn set_and_wait_update(
        &mut self,
        flags: CtlFlags,
        max_polls: NonZeroU32,
    ) -> Result<(), Error> {
        let flag = flags.bits();
        let persistent = self.read(ADDR_CTL_FLAG);
        self.write(ADDR_CTL_FLAG, persistent | flag);
        for _ in 0..max_polls.get() {
            if (self.read(ADDR_CTL_FLAG) & flag) == 0 {
                let result = self.read(ADDR_SILENCER_SET_RESULT);
                return if (result & flag) == 0 {
                    Ok(())
                } else if result & (1 << SET_RESULT_TRANSITION_INVALID_BIT) != 0 {
                    Err(Error::InvalidTransitionMode)
                } else {
                    Err(Error::InvalidSilencerSetting)
                };
            }
        }
        Err(Error::FpgaTimeout)
    }

    fn write_switch(&mut self, reg: u16, value: u16) {
        self.bram.port.memory_barrier();
        self.write(reg, value);
        self.bram.port.memory_barrier();
    }
}

#[derive(Default)]
pub struct WatchdogState {
    pub fired: bool,
    pub count: u8,
}

pub struct Watchdog<'a, P: Port> {
    bram: Bram<'a, P>,
}

pub fn watchdog<P: Port>(port: &mut P) -> Watchdog<'_, P> {
    Watchdog {
        bram: Bram {
            port,
            select: BramSelect::Watchdog,
        },
    }
}

impl<P: Port> Watchdog<'_, P> {
    pub fn kick(&mut self) {
        self.bram.write(ADDR_WATCHDOG_KICK, 0);
    }

    pub fn set_timeout(&mut self, timeout: Option<Duration>) {
        let ms = timeout.map_or(0, |timeout| {
            u16::try_from(timeout.as_millis()).unwrap_or(u16::MAX)
        });
        self.bram.write(ADDR_WATCHDOG_TIMEOUT, ms);
    }

    pub fn release(&mut self) {
        self.bram.write(ADDR_WATCHDOG_RELEASE, 0);
    }

    #[must_use]
    pub fn state(&mut self) -> WatchdogState {
        if !controller(self.bram.port)
            .functions()
            .contains(FunctionBits::CPU_WATCHDOG)
        {
            return WatchdogState::default();
        }
        let raw = self.bram.read(ADDR_WATCHDOG_STATE);
        WatchdogState {
            fired: raw & 1 != 0,
            count: (raw >> 8) as u8,
        }
    }
}

pub struct SyncPs<'a, P: Port> {
    bram: Bram<'a, P>,
}

pub fn sync_ps<P: Port>(port: &mut P) -> SyncPs<'_, P> {
    SyncPs {
        bram: Bram {
            port,
            select: BramSelect::SyncPs,
        },
    }
}

impl<P: Port> SyncPs<'_, P> {
    pub fn apply(&mut self, config: &FpgaConfig) {
        self.bram.write(ADDR_SYNC_PS_KP, config.sync_kp);
        self.bram.write(ADDR_SYNC_PS_KI, config.sync_ki);
        self.bram.write(ADDR_SYNC_PS_KF, config.sync_kf);
        self.bram
            .write(ADDR_SYNC_PS_CTL, u16::from(config.sync_phase_lock));
    }

    fn wait_stats_idle(&mut self, max_polls: NonZeroU32) -> Result<(), Error> {
        for _ in 0..max_polls.get() {
            if self.bram.read(ADDR_SYNC_PS_STATS_CTL) & 1 == 0 {
                return Ok(());
            }
        }
        Err(Error::FpgaTimeout)
    }

    fn command_stats(&mut self, command: u16, max_polls: NonZeroU32) -> Result<(), Error> {
        self.wait_stats_idle(max_polls)?;
        self.bram.write(ADDR_SYNC_PS_STATS_CTL, command);
        self.wait_stats_idle(max_polls)
    }

    pub fn snapshot_stats(
        &mut self,
        clear: bool,
        max_polls: NonZeroU32,
    ) -> Result<SyncPsStats, Error> {
        self.command_stats(1 | (u16::from(clear) << 1), max_polls)?;
        Ok(SyncPsStats {
            diff_min: I16::new(self.bram.read(ADDR_SYNC_PS_DIFF_MIN).cast_signed()),
            diff_max: I16::new(self.bram.read(ADDR_SYNC_PS_DIFF_MAX).cast_signed()),
            diff_outliers: U16::new(self.bram.read(ADDR_SYNC_PS_DIFF_OUTLIERS)),
            rate_min: I16::new(self.bram.read(ADDR_SYNC_PS_RATE_MIN).cast_signed()),
            rate_max: I16::new(self.bram.read(ADDR_SYNC_PS_RATE_MAX).cast_signed()),
            fine_exits: U16::new(self.bram.read(ADDR_SYNC_PS_FINE_EXITS)),
        })
    }

    pub fn start_tdc_capture(
        &mut self,
        source: TdcSource,
        max_polls: NonZeroU32,
    ) -> Result<(), Error> {
        self.command_stats(0b100 | (u16::from(source.as_u8()) << 3), max_polls)
    }

    #[must_use]
    pub fn read_tdc(&mut self, chunk: u8) -> TdcChunk {
        let status = self.bram.read(ADDR_SYNC_PS_STATS_CTL);
        let base = ADDR_SYNC_PS_TDC_BUF + u16::from(chunk) * TDC_CHUNK_SAMPLES as u16;
        TdcChunk {
            capturing: status & 0b010 != 0,
            locked: status & 0b100 != 0,
            samples: core::array::from_fn(|i| U16::new(self.bram.read(base + i as u16))),
        }
    }
}

pub struct Ram<'a, P: Port> {
    bram: Bram<'a, P>,
    wr_bank_reg: u16,
    wr_page_reg: u16,
}

pub fn mod_ram<P: Port>(port: &mut P) -> Ram<'_, P> {
    Ram {
        bram: Bram {
            port,
            select: BramSelect::Mod,
        },
        wr_bank_reg: ADDR_MOD_MEM_WR_BANK,
        wr_page_reg: ADDR_MOD_MEM_WR_PAGE,
    }
}

pub fn emission_ram<P: Port>(port: &mut P) -> Ram<'_, P> {
    Ram {
        bram: Bram {
            port,
            select: BramSelect::Emission,
        },
        wr_bank_reg: ADDR_PATTERN_MEM_WR_BANK,
        wr_page_reg: ADDR_PATTERN_MEM_WR_PAGE,
    }
}

impl<P: Port> Ram<'_, P> {
    pub fn write(&mut self, bank: u8, offset: u32, src: &[u8]) {
        self.write_words(bank, offset, le_words(src, 0));
    }

    pub fn write_words(&mut self, bank: u8, offset: u32, words: impl Iterator<Item = u16>) {
        self.write_at(bank, (offset..).zip(words));
    }

    pub fn write_at(&mut self, bank: u8, words: impl Iterator<Item = (u32, u16)>) {
        let mut words = words.peekable();
        let Some(&(first, _)) = words.peek() else {
            return;
        };
        let wr_bank_reg = self.wr_bank_reg;
        let wr_page_reg = self.wr_page_reg;
        controller(self.bram.port).write_switch(wr_bank_reg, u16::from(bank));
        let mut page = first / FPGA_PAGE_WORDS;
        controller(self.bram.port).write_switch(wr_page_reg, page as u16);
        words.for_each(|(word_idx, word)| {
            let p = word_idx / FPGA_PAGE_WORDS;
            if p != page {
                page = p;
                controller(self.bram.port).write_switch(wr_page_reg, page as u16);
            }
            self.bram.write((word_idx % FPGA_PAGE_WORDS) as u16, word);
        });
    }
}

pub fn init<P: Port>(port: &mut P) -> Result<(), Error> {
    for bank in 0..NUM_BANKS as u8 {
        mod_ram(port).write_words(bank, 0, core::iter::once(0xFFFF));
        emission_ram(port).write_words(bank, 0, core::iter::repeat_n(0, NUM_TRANSDUCERS));
    }
    phase_corr(port).write_words(core::iter::repeat_n(0, PHASE_CORR_WORDS));
    output_mask(port).write_words(core::iter::repeat_n(0xFFFF, OUTPUT_MASK_WORDS));
    pwe_table(port).write_words(PWE_DEFAULT_TABLE.iter().copied());

    let mut ctl = controller(port);
    let gate = CtlFlags::from_bits_retain(ctl.read(ADDR_CTL_FLAG)) & CtlFlags::FAILSAFE;
    ctl.write(ADDR_CTL_FLAG, gate.bits());
    ctl.write(
        ADDR_SILENCER_UPDATE_RATE_INTENSITY,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    ctl.write(
        ADDR_SILENCER_UPDATE_RATE_PHASE,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    ctl.write(ADDR_SILENCER_FLAG, 0);
    ctl.write(
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
    );
    ctl.write(
        ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    );
    ctl.write(
        ADDR_MOD_TRANSITION_MODE,
        TransitionModeKind::Immediate as u16,
    );
    ctl.write_u64(ADDR_MOD_TRANSITION_VALUE_0, 0);
    ctl.write(ADDR_MOD_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u8 {
        ctl.write(ADDR_MOD_CYCLE0 + u16::from(bank), 1);
        ctl.write(ADDR_MOD_FREQ_DIV0 + u16::from(bank), 0xFFFF);
        ctl.write(ADDR_MOD_REP0 + u16::from(bank), REP_INFINITE);
    }
    ctl.write(
        ADDR_PATTERN_TRANSITION_MODE,
        TransitionModeKind::Immediate as u16,
    );
    ctl.write_u64(ADDR_PATTERN_TRANSITION_VALUE_0, 0);
    ctl.write(ADDR_PATTERN_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u8 {
        ctl.write(
            ADDR_PATTERN_MODE0 + u16::from(bank),
            EmissionType::Raw as u16,
        );
        ctl.write(ADDR_PATTERN_CYCLE0 + u16::from(bank), 0);
        ctl.write(ADDR_PATTERN_FREQ_DIV0 + u16::from(bank), 0xFFFF);
        ctl.write(ADDR_PATTERN_REP0 + u16::from(bank), REP_INFINITE);
    }

    for i in 0..DEBUG_VALUE_WORDS {
        ctl.write(ADDR_DEBUG_VALUE0_0 + i, 0);
    }

    ctl.set_and_wait_update(CtlFlags::SILENCER_SET, FPGA_WAIT_UPDATE_MAX_POLLS)?;
    ctl.set_and_wait_update(
        CtlFlags::MOD_SET | CtlFlags::PATTERN_SET | CtlFlags::DEBUG_SET,
        FPGA_WAIT_UPDATE_MAX_POLLS,
    )?;
    if !gate.is_empty() {
        ctl.write(ADDR_CTL_FLAG, 0);
    }

    watchdog(port).release();
    sync_ps(port).apply(&FpgaConfig::DEFAULT);

    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use core::num::NonZeroU32;

    use super::{FPGA_PAGE_WORDS, controller, mod_ram};
    use crate::fpga_params::{ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, CtlFlags};
    use crate::proto::Error;
    use crate::test_utils::mock::Harness;

    #[test]
    fn a_latch_is_set_and_awaited_without_a_barrier() {
        let mut h = Harness::new();
        h.port.barriers = 0;
        let latched = h.latch_count(CtlFlags::SILENCER_SET);
        assert_eq!(
            controller(&mut h.port).set_and_wait_update(CtlFlags::SILENCER_SET, NonZeroU32::MIN),
            Ok(())
        );
        assert_eq!(h.latch_count(CtlFlags::SILENCER_SET), latched + 1);
        assert_eq!(h.port.barriers, 0);
    }

    #[test]
    fn a_stuck_latch_is_polled_the_given_number_of_times() {
        let mut h = Harness::new();
        h.port.latch_stuck = true;
        let reads = h.port.ctl_flag_reads;
        assert_eq!(
            controller(&mut h.port)
                .set_and_wait_update(CtlFlags::SYNC_SET, NonZeroU32::new(5).unwrap()),
            Err(Error::FpgaTimeout)
        );
        assert_eq!(h.port.ctl_flag_reads - reads, 1 + 5);
    }

    #[test]
    fn every_bank_and_page_switch_of_a_ram_write_sits_between_barriers() {
        let mut h = Harness::new();
        h.port.ram_switches.clear();
        h.port.barriers = 0;
        mod_ram(&mut h.port).write_words(1, FPGA_PAGE_WORDS - 1, [0x1234, 0x5678].into_iter());
        assert_eq!(
            h.port.ram_switches,
            [
                (ADDR_MOD_MEM_WR_BANK, 1),
                (ADDR_MOD_MEM_WR_PAGE, 0),
                (ADDR_MOD_MEM_WR_PAGE, 1)
            ]
        );
        assert_eq!(h.port.barriers, 6);
        assert_eq!(h.mod_word(1, FPGA_PAGE_WORDS as usize - 1), 0x1234);
        assert_eq!(h.mod_word(1, FPGA_PAGE_WORDS as usize), 0x5678);
    }
}
