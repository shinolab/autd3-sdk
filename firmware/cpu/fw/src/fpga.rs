use core::num::NonZeroU32;

use autd3_cpu_wire::cpu_params::PWE_DEFAULT_TABLE;

use crate::fpga_params::{
    ADDR_CTL_FLAG, ADDR_DEBUG_VALUE0_0, ADDR_FUNCTION_BITS, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0,
    ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
    ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0, ADDR_PATTERN_CYCLE0,
    ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, ADDR_PATTERN_MODE0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
    ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG, ADDR_SILENCER_SET_RESULT,
    ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE, BramSelect, CtlFlags,
    FunctionBits, NUM_BANKS, NUM_TRANSDUCERS,
};
pub use crate::fpga_params::{PWE_TABLE_SIZE, REP_INFINITE};
use crate::port::Port;
use crate::proto::{Error, OUTPUT_MASK_WORDS};

pub const FPGA_PAGE_WORDS: u32 = 16384;

pub use autd3_cpu_wire::payload::{EmissionType, TransitionMode};

#[must_use]
pub const fn sys_time_ticks(ns: u64) -> u64 {
    ns / 3125 * 64
}

#[must_use]
pub fn transition_register_value(transition_mode: TransitionMode, transition_value: u64) -> u64 {
    if transition_mode == TransitionMode::SysTime {
        sys_time_ticks(transition_value)
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

pub fn write<P: Port>(port: &mut P, select: BramSelect, addr: u16, value: u16) {
    port.fpga_write((u16::from(select.as_u8()) << 8) | addr, value);
}

pub fn read<P: Port>(port: &mut P, select: BramSelect, addr: u16) -> u16 {
    port.fpga_read((u16::from(select.as_u8()) << 8) | addr)
}

pub fn write_ctl<P: Port>(port: &mut P, addr: u16, value: u16) {
    write(port, BramSelect::Controller, addr, value);
}

pub fn read_ctl<P: Port>(port: &mut P, addr: u16) -> u16 {
    read(port, BramSelect::Controller, addr)
}

fn write_switch<P: Port>(port: &mut P, reg: u16, value: u16) {
    port.memory_barrier();
    write_ctl(port, reg, value);
    port.memory_barrier();
}

pub fn functions<P: Port>(port: &mut P) -> FunctionBits {
    match read_ctl(port, ADDR_FUNCTION_BITS) as u8 {
        0xFF => FunctionBits::empty(),
        bits => FunctionBits::from_bits_retain(bits),
    }
}

pub fn set_and_wait_update<P: Port>(
    port: &mut P,
    flags: CtlFlags,
    max_polls: NonZeroU32,
) -> Result<(), Error> {
    let flag = flags.bits();
    let persistent = read_ctl(port, ADDR_CTL_FLAG);
    write_ctl(port, ADDR_CTL_FLAG, persistent | flag);
    port.memory_barrier();
    for _ in 0..max_polls.get() {
        if (read_ctl(port, ADDR_CTL_FLAG) & flag) == 0 {
            return if (read_ctl(port, ADDR_SILENCER_SET_RESULT) & flag) == 0 {
                Ok(())
            } else {
                Err(Error::InvalidSilencerSetting)
            };
        }
    }
    Err(Error::FpgaTimeout)
}

pub fn write_u64<P: Port>(port: &mut P, addr: u16, value: u64) {
    for i in 0..4u16 {
        write_ctl(port, addr + i, (value >> (16 * i)) as u16);
    }
}

pub fn validate_transition_mode<P: Port>(
    port: &mut P,
    rep: u16,
    transition_mode: TransitionMode,
    transition_value: u64,
    margin_ns: u64,
) -> Result<(), Error> {
    let loop_compatible = if rep == REP_INFINITE {
        matches!(
            transition_mode,
            TransitionMode::Immediate | TransitionMode::Ext
        )
    } else {
        matches!(
            transition_mode,
            TransitionMode::SyncIdx | TransitionMode::SysTime | TransitionMode::Gpio
        )
    };
    if !loop_compatible {
        return Err(Error::InvalidTransitionMode);
    }
    if transition_mode == TransitionMode::SysTime
        && port
            .sys_time()
            .is_none_or(|now| transition_value < now + margin_ns)
    {
        return Err(Error::MissTransitionTime);
    }
    Ok(())
}

pub struct Ram {
    select: BramSelect,
    wr_bank_reg: u16,
    wr_page_reg: u16,
}

pub const MOD_RAM: Ram = Ram {
    select: BramSelect::Mod,
    wr_bank_reg: ADDR_MOD_MEM_WR_BANK,
    wr_page_reg: ADDR_MOD_MEM_WR_PAGE,
};

pub const EMISSION_RAM: Ram = Ram {
    select: BramSelect::Emission,
    wr_bank_reg: ADDR_PATTERN_MEM_WR_BANK,
    wr_page_reg: ADDR_PATTERN_MEM_WR_PAGE,
};

pub fn le_words(data: &[u8], pad: u8) -> impl Iterator<Item = u16> {
    let (pairs, tail) = data.as_chunks::<2>();
    pairs
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .chain(tail.first().map(|byte| u16::from_le_bytes([*byte, pad])))
}

pub fn write_ram<P: Port>(port: &mut P, ram: &Ram, bank: u8, offset: u32, src: &[u8]) {
    write_ram_words(port, ram, bank, offset, le_words(src, 0));
}

pub fn write_ram_interleaved<P: Port>(
    port: &mut P,
    ram: &Ram,
    bank: u8,
    offset: u32,
    lo: &[u8; NUM_TRANSDUCERS],
    hi: &[u8; NUM_TRANSDUCERS],
) {
    write_ram_words(
        port,
        ram,
        bank,
        offset,
        lo.iter()
            .zip(hi)
            .map(|(&lo, &hi)| u16::from_le_bytes([lo, hi])),
    );
}

pub fn write_ram_words<P: Port>(
    port: &mut P,
    ram: &Ram,
    bank: u8,
    offset: u32,
    words: impl Iterator<Item = u16>,
) {
    let mut words = words.peekable();
    if words.peek().is_none() {
        return;
    }
    write_switch(port, ram.wr_bank_reg, u16::from(bank));
    let mut page = offset / FPGA_PAGE_WORDS;
    write_switch(port, ram.wr_page_reg, page as u16);
    for (word_idx, word) in (offset..).zip(words) {
        let p = word_idx / FPGA_PAGE_WORDS;
        if p != page {
            page = p;
            write_switch(port, ram.wr_page_reg, page as u16);
        }
        write(port, ram.select, (word_idx % FPGA_PAGE_WORDS) as u16, word);
    }
}

fn init_silencer<P: Port>(port: &mut P) {
    write_ctl(
        port,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    write_ctl(
        port,
        ADDR_SILENCER_UPDATE_RATE_PHASE,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    write_ctl(port, ADDR_SILENCER_FLAG, 0);
    write_ctl(
        port,
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
    );
    write_ctl(
        port,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    );
}

fn init_mod<P: Port>(port: &mut P) {
    write_ctl(
        port,
        ADDR_MOD_TRANSITION_MODE,
        TransitionMode::SyncIdx as u16,
    );
    write_u64(port, ADDR_MOD_TRANSITION_VALUE_0, 0);
    write_ctl(port, ADDR_MOD_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u8 {
        write_ctl(port, ADDR_MOD_CYCLE0 + u16::from(bank), 1);
        write_ctl(port, ADDR_MOD_FREQ_DIV0 + u16::from(bank), 0xFFFF);
        write_ctl(port, ADDR_MOD_REP0 + u16::from(bank), REP_INFINITE);
        write_ram_words(port, &MOD_RAM, bank, 0, core::iter::once(0xFFFF));
    }
}

fn init_pattern<P: Port>(port: &mut P) {
    write_ctl(
        port,
        ADDR_PATTERN_TRANSITION_MODE,
        TransitionMode::SyncIdx as u16,
    );
    write_u64(port, ADDR_PATTERN_TRANSITION_VALUE_0, 0);
    write_ctl(port, ADDR_PATTERN_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u8 {
        write_ctl(
            port,
            ADDR_PATTERN_MODE0 + u16::from(bank),
            EmissionType::Raw as u16,
        );
        write_ctl(port, ADDR_PATTERN_CYCLE0 + u16::from(bank), 0);
        write_ctl(port, ADDR_PATTERN_FREQ_DIV0 + u16::from(bank), 0xFFFF);
        write_ctl(port, ADDR_PATTERN_REP0 + u16::from(bank), REP_INFINITE);
        write_ram_words(
            port,
            &EMISSION_RAM,
            bank,
            0,
            core::iter::repeat_n(0, NUM_TRANSDUCERS),
        );
    }
}

fn init_tables<P: Port>(port: &mut P) {
    for i in 0..PHASE_CORR_WORDS as u16 {
        write(port, BramSelect::PhaseCorr, i, 0);
    }
    for i in 0..OUTPUT_MASK_WORDS as u16 {
        write(port, BramSelect::OutputMask, i, 0xFFFF);
    }

    for (i, v) in PWE_DEFAULT_TABLE.iter().enumerate() {
        write(port, BramSelect::PweTable, i as u16, *v);
    }

    for i in 0..DEBUG_VALUE_WORDS {
        write_ctl(port, ADDR_DEBUG_VALUE0_0 + i, 0);
    }
}

pub fn init<P: Port>(port: &mut P, max_polls: NonZeroU32) -> Result<(), Error> {
    let gate = CtlFlags::from_bits_retain(read_ctl(port, ADDR_CTL_FLAG)) & CtlFlags::FAILSAFE;
    write_ctl(port, ADDR_CTL_FLAG, gate.bits());
    init_silencer(port);
    init_mod(port);
    init_pattern(port);
    init_tables(port);

    set_and_wait_update(port, CtlFlags::SILENCER_SET, max_polls)?;
    set_and_wait_update(
        port,
        CtlFlags::MOD_SET | CtlFlags::PATTERN_SET | CtlFlags::DEBUG_SET,
        max_polls,
    )?;
    if !gate.is_empty() {
        write_ctl(port, ADDR_CTL_FLAG, 0);
    }
    Ok(())
}
