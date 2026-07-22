use crate::params::{
    ADDR_CTL_FLAG, ADDR_DEBUG_VALUE0_0, ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_MEM_WR_BANK,
    ADDR_MOD_MEM_WR_PAGE, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK, ADDR_MOD_TRANSITION_MODE,
    ADDR_MOD_TRANSITION_VALUE_0, ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0,
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, ADDR_PATTERN_MODE0, ADDR_PATTERN_REP0,
    ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0,
    ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
    ADDR_SILENCER_FLAG, ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE,
    BRAM_CNT_SELECT_OUTPUT_MASK, BRAM_CNT_SELECT_PHASE_CORR, BRAM_SELECT_CONTROLLER,
    BRAM_SELECT_EMISSION, BRAM_SELECT_MOD, BRAM_SELECT_PWE_TABLE, CTL_FLAG_DEBUG_SET,
    CTL_FLAG_MOD_SET, CTL_FLAG_PATTERN_SET, CTL_FLAG_SILENCER_SET, EMISSION_TYPE_FOCI,
    EMISSION_TYPE_RAW, NUM_BANKS, NUM_TRANSDUCERS, TRANSITION_MODE_EXT, TRANSITION_MODE_GPIO,
    TRANSITION_MODE_IMMEDIATE, TRANSITION_MODE_SYNC_IDX, TRANSITION_MODE_SYS_TIME,
};
pub use crate::params::{PWE_TABLE_SIZE, REP_INFINITE};
use crate::port::Port;
use crate::proto::{Error, Mode, OUTPUT_MASK_WORDS, wire_enum};

pub const FPGA_PAGE_WORDS: u32 = 16384;

wire_enum! {
    pub enum TransitionMode {
        SyncIdx = TRANSITION_MODE_SYNC_IDX,
        SysTime = TRANSITION_MODE_SYS_TIME,
        Gpio = TRANSITION_MODE_GPIO,
        Ext = TRANSITION_MODE_EXT,
        Immediate = TRANSITION_MODE_IMMEDIATE,
    }
}

wire_enum! {
    pub enum EmissionType {
        Foci = EMISSION_TYPE_FOCI,
        Raw = EMISSION_TYPE_RAW,
    }
}

pub const SYS_TIME_TRANSITION_MARGIN_NS: u64 = 10_000_000;

#[must_use]
pub fn sys_time_margin_ns(margin_ns: u32) -> u64 {
    if margin_ns == 0 {
        SYS_TIME_TRANSITION_MARGIN_NS
    } else {
        u64::from(margin_ns)
    }
}

pub use autd3_cpu_wire::payload::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE,
};
pub const PHASE_CORR_WORDS: usize = NUM_TRANSDUCERS.div_ceil(2);
pub const DEBUG_VALUE_WORDS: u16 = 16;

pub const FPGA_WAIT_UPDATE_MAX_POLLS: u32 = 1_000_000;
pub const FPGA_WAIT_UPDATE_MAX_POLLS_INLINE: u32 = 1_000;

pub fn write<P: Port>(port: &mut P, select: u8, addr: u16, value: u16) {
    port.fpga_write((u16::from(select) << 14) | (addr & 0x3FFF), value);
}

pub fn read<P: Port>(port: &mut P, select: u8, addr: u16) -> u16 {
    port.fpga_read((u16::from(select) << 14) | (addr & 0x3FFF))
}

fn write_switch<P: Port>(port: &mut P, reg: u16, value: u16) {
    port.memory_barrier();
    write(port, BRAM_SELECT_CONTROLLER, reg, value);
    port.memory_barrier();
}

pub fn set_and_wait_update<P: Port>(port: &mut P, mode: Mode, flag: u16) -> Result<(), Error> {
    let max_polls = match mode {
        Mode::LowLatency => FPGA_WAIT_UPDATE_MAX_POLLS_INLINE,
        Mode::Fifo => FPGA_WAIT_UPDATE_MAX_POLLS,
    };
    let persistent = read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG);
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_CTL_FLAG,
        persistent | flag,
    );
    port.memory_barrier();
    for _ in 0..max_polls {
        if (read(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG) & flag) == 0 {
            return Ok(());
        }
    }
    Err(Error::FpgaTimeout)
}

pub fn write_u64<P: Port>(port: &mut P, addr: u16, value: u64) {
    for i in 0..4u16 {
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            addr + i,
            (value >> (16 * i)) as u16,
        );
    }
}

pub fn write_change_bank<P: Port>(
    port: &mut P,
    req_rd_bank_addr: u16,
    transition_mode_addr: u16,
    transition_value_addr: u16,
    bank: u8,
    transition_mode: TransitionMode,
    transition_value: u64,
) {
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        transition_mode_addr,
        transition_mode as u16,
    );
    write_u64(port, transition_value_addr, transition_value);
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        req_rd_bank_addr,
        u16::from(bank),
    );
}

#[must_use]
pub fn transition_mode_violates_loop(rep: u16, transition_mode: TransitionMode) -> bool {
    if rep == REP_INFINITE {
        !matches!(
            transition_mode,
            TransitionMode::Immediate | TransitionMode::Ext
        )
    } else {
        !matches!(
            transition_mode,
            TransitionMode::SyncIdx | TransitionMode::SysTime | TransitionMode::Gpio
        )
    }
}

pub fn write_ram<P: Port>(
    port: &mut P,
    select: u8,
    wr_bank_reg: u16,
    wr_page_reg: u16,
    bank: u8,
    offset: u32,
    src: &[u8],
) {
    write_switch(port, wr_bank_reg, u16::from(bank));
    let mut page = offset / FPGA_PAGE_WORDS;
    write_switch(port, wr_page_reg, page as u16);
    let n_words = src.len().div_ceil(2);
    for i in 0..n_words {
        let word_idx = offset + i as u32;
        let p = word_idx / FPGA_PAGE_WORDS;
        if p != page {
            page = p;
            write_switch(port, wr_page_reg, page as u16);
        }
        let lo = u16::from(src[2 * i]);
        let hi = if 2 * i + 1 < src.len() {
            u16::from(src[2 * i + 1])
        } else {
            0
        };
        write(
            port,
            select,
            (word_idx % FPGA_PAGE_WORDS) as u16,
            lo | (hi << 8),
        );
    }
}

const ASIN_TABLE: [u8; PWE_TABLE_SIZE] = [
    0x00, 0x01, 0x01, 0x02, 0x03, 0x03, 0x04, 0x04, 0x05, 0x06, 0x06, 0x07, 0x08, 0x08, 0x09, 0x0a,
    0x0a, 0x0b, 0x0c, 0x0c, 0x0d, 0x0d, 0x0e, 0x0f, 0x0f, 0x10, 0x11, 0x11, 0x12, 0x13, 0x13, 0x14,
    0x15, 0x15, 0x16, 0x16, 0x17, 0x18, 0x18, 0x19, 0x1a, 0x1a, 0x1b, 0x1c, 0x1c, 0x1d, 0x1e, 0x1e,
    0x1f, 0x20, 0x20, 0x21, 0x21, 0x22, 0x23, 0x23, 0x24, 0x25, 0x25, 0x26, 0x27, 0x27, 0x28, 0x29,
    0x29, 0x2a, 0x2b, 0x2b, 0x2c, 0x2d, 0x2d, 0x2e, 0x2f, 0x2f, 0x30, 0x31, 0x31, 0x32, 0x33, 0x33,
    0x34, 0x35, 0x35, 0x36, 0x37, 0x37, 0x38, 0x39, 0x39, 0x3a, 0x3b, 0x3b, 0x3c, 0x3d, 0x3e, 0x3e,
    0x3f, 0x40, 0x40, 0x41, 0x42, 0x42, 0x43, 0x44, 0x44, 0x45, 0x46, 0x47, 0x47, 0x48, 0x49, 0x49,
    0x4a, 0x4b, 0x4c, 0x4c, 0x4d, 0x4e, 0x4e, 0x4f, 0x50, 0x51, 0x51, 0x52, 0x53, 0x53, 0x54, 0x55,
    0x56, 0x56, 0x57, 0x58, 0x59, 0x59, 0x5a, 0x5b, 0x5c, 0x5c, 0x5d, 0x5e, 0x5f, 0x5f, 0x60, 0x61,
    0x62, 0x63, 0x63, 0x64, 0x65, 0x66, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x6a, 0x6b, 0x6c, 0x6d, 0x6e,
    0x6f, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x7a, 0x7b,
    0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a,
    0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a,
    0x9b, 0x9d, 0x9e, 0x9f, 0xa0, 0xa1, 0xa2, 0xa3, 0xa5, 0xa6, 0xa7, 0xa8, 0xaa, 0xab, 0xac, 0xad,
    0xaf, 0xb0, 0xb2, 0xb3, 0xb4, 0xb6, 0xb7, 0xb9, 0xba, 0xbc, 0xbd, 0xbf, 0xc1, 0xc2, 0xc4, 0xc6,
    0xc8, 0xca, 0xcc, 0xce, 0xd0, 0xd2, 0xd5, 0xd7, 0xda, 0xdd, 0xe0, 0xe3, 0xe7, 0xec, 0xf2, 0x00,
];

fn init_silencer<P: Port>(port: &mut P) {
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_SILENCER_UPDATE_RATE_PHASE,
        SILENCER_DEFAULT_UPDATE_RATE,
    );
    write(port, BRAM_SELECT_CONTROLLER, ADDR_SILENCER_FLAG, 0);
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
    );
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    );
}

fn init_mod<P: Port>(port: &mut P) {
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_MOD_TRANSITION_MODE,
        TransitionMode::SyncIdx as u16,
    );
    write_u64(port, ADDR_MOD_TRANSITION_VALUE_0, 0);
    write(port, BRAM_SELECT_CONTROLLER, ADDR_MOD_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u16 {
        write(port, BRAM_SELECT_CONTROLLER, ADDR_MOD_CYCLE0 + bank, 1);
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_FREQ_DIV0 + bank,
            0xFFFF,
        );
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + bank,
            REP_INFINITE,
        );
        write_switch(port, ADDR_MOD_MEM_WR_BANK, bank);
        write_switch(port, ADDR_MOD_MEM_WR_PAGE, 0);
        write(port, BRAM_SELECT_MOD, 0, 0xFFFF);
    }
}

fn init_pattern<P: Port>(port: &mut P) {
    write(
        port,
        BRAM_SELECT_CONTROLLER,
        ADDR_PATTERN_TRANSITION_MODE,
        TransitionMode::SyncIdx as u16,
    );
    write_u64(port, ADDR_PATTERN_TRANSITION_VALUE_0, 0);
    write(port, BRAM_SELECT_CONTROLLER, ADDR_PATTERN_REQ_RD_BANK, 0);
    for bank in 0..NUM_BANKS as u16 {
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_MODE0 + bank,
            EmissionType::Raw as u16,
        );
        write(port, BRAM_SELECT_CONTROLLER, ADDR_PATTERN_CYCLE0 + bank, 0);
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_FREQ_DIV0 + bank,
            0xFFFF,
        );
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + bank,
            REP_INFINITE,
        );
        write_switch(port, ADDR_PATTERN_MEM_WR_BANK, bank);
        write_switch(port, ADDR_PATTERN_MEM_WR_PAGE, 0);
        for i in 0..NUM_TRANSDUCERS as u16 {
            write(port, BRAM_SELECT_EMISSION, i, 0);
        }
    }
}

fn init_tables<P: Port>(port: &mut P) {
    for i in 0..PHASE_CORR_WORDS as u16 {
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_PHASE_CORR) << 8) | i,
            0,
        );
    }
    for i in 0..OUTPUT_MASK_WORDS as u16 {
        write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_OUTPUT_MASK) << 8) | i,
            0xFFFF,
        );
    }

    for (i, v) in ASIN_TABLE.iter().enumerate() {
        write(port, BRAM_SELECT_PWE_TABLE, i as u16, u16::from(*v));
    }
    write(
        port,
        BRAM_SELECT_PWE_TABLE,
        PWE_TABLE_SIZE as u16 - 1,
        0x100,
    );

    for i in 0..DEBUG_VALUE_WORDS {
        write(port, BRAM_SELECT_CONTROLLER, ADDR_DEBUG_VALUE0_0 + i, 0);
    }
}

pub fn init<P: Port>(port: &mut P, mode: Mode) -> Result<(), Error> {
    write(port, BRAM_SELECT_CONTROLLER, ADDR_CTL_FLAG, 0);
    init_silencer(port);
    init_mod(port);
    init_pattern(port);
    init_tables(port);

    let mut result = Ok(());
    for flag in [
        CTL_FLAG_MOD_SET,
        CTL_FLAG_PATTERN_SET,
        CTL_FLAG_SILENCER_SET,
        CTL_FLAG_DEBUG_SET,
    ] {
        if set_and_wait_update(port, mode, flag).is_err() {
            result = Err(Error::FpgaTimeout);
        }
    }
    result
}
