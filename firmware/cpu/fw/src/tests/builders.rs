use crate::fpga::REP_INFINITE;
use crate::proto::{
    CHANGE_BANK_OFFSET_BANK, CHANGE_BANK_OFFSET_TRANSITION_MODE,
    CHANGE_BANK_OFFSET_TRANSITION_VALUE, CMD_CHANGE_MOD_BANK, CMD_CHANGE_PATTERN_BANK,
    CMD_CONFIG_MOD, CMD_CONFIG_PATTERN, CMD_EMULATE_GPIO_IN, CMD_FORCE_FAN, CMD_SET_GPIO_OUT,
    CMD_SET_OUTPUT_MASK, CMD_SET_PHASE_CORR, CMD_SET_PWE, CMD_SET_SILENCER, CMD_WRITE_MOD_BUFFER,
    CMD_WRITE_PATTERN_BUFFER, CMD_WRITE_PATTERN_COMPRESSED, CMD_XOR_HASH,
    EM_COMPRESSED_OFFSET_BANK, EM_COMPRESSED_OFFSET_COUNT, EM_COMPRESSED_OFFSET_DATA,
    EM_COMPRESSED_OFFSET_FORMAT, EM_COMPRESSED_OFFSET_OFFSET, EM_CONFIG_OFFSET_BANK,
    EM_CONFIG_OFFSET_DIVIDER, EM_CONFIG_OFFSET_NUM_FOCI, EM_CONFIG_OFFSET_REP,
    EM_CONFIG_OFFSET_SIZE, EM_CONFIG_OFFSET_SOUND_SPEED, EM_CONFIG_OFFSET_TYPE,
    EM_WRITE_OFFSET_BANK, EM_WRITE_OFFSET_DATA, EM_WRITE_OFFSET_DATA_LEN, EM_WRITE_OFFSET_OFFSET,
    FORCE_FAN_OFFSET_VALUE, GPIO_IN_OFFSET_FLAG, GPIO_OUT_OFFSET_DATA, MOD_CONFIG_OFFSET_BANK,
    MOD_CONFIG_OFFSET_DIVIDER, MOD_CONFIG_OFFSET_REP, MOD_CONFIG_OFFSET_SIZE,
    MOD_WRITE_OFFSET_BANK, MOD_WRITE_OFFSET_DATA, MOD_WRITE_OFFSET_DATA_LEN,
    MOD_WRITE_OFFSET_OFFSET, OUTPUT_MASK_OFFSET_DATA, PHASE_CORR_OFFSET_DATA, PWE_OFFSET_DATA,
    SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY, SILENCER_OFFSET_COMPLETION_STEPS_PHASE,
    SILENCER_OFFSET_FLAG, SILENCER_OFFSET_UPDATE_RATE_INTENSITY, SILENCER_OFFSET_UPDATE_RATE_PHASE,
    XOR_HASH_OFFSET_DATA, XOR_HASH_OFFSET_DATA_LEN, XOR_HASH_OFFSET_SLEEP_MS,
};
use crate::tests::mock::Frame;

pub(crate) fn xor_hash_ok(seq: u8, sleep_ms: u16, data: &[u8]) -> Frame {
    let mut f = Frame::new(seq, CMD_XOR_HASH);
    f.put_u16(XOR_HASH_OFFSET_SLEEP_MS, sleep_ms);
    let checksum = data.iter().fold(0u8, |h, b| h ^ b);
    f.put_u16(XOR_HASH_OFFSET_DATA_LEN, (data.len() + 1) as u16);
    f.payload()[XOR_HASH_OFFSET_DATA..XOR_HASH_OFFSET_DATA + data.len()].copy_from_slice(data);
    f.payload()[XOR_HASH_OFFSET_DATA + data.len()] = checksum;
    f
}

pub(crate) fn xor_hash_bad(seq: u8, data: &[u8]) -> Frame {
    let mut f = Frame::new(seq, CMD_XOR_HASH);
    f.put_u16(XOR_HASH_OFFSET_SLEEP_MS, 0);
    f.put_u16(XOR_HASH_OFFSET_DATA_LEN, data.len() as u16);
    f.payload()[XOR_HASH_OFFSET_DATA..XOR_HASH_OFFSET_DATA + data.len()].copy_from_slice(data);
    f
}

pub(crate) fn write_pattern_buffer(seq: u8, bank: u8, offset_words: u32, words: &[u16]) -> Frame {
    let mut f = Frame::new(seq, CMD_WRITE_PATTERN_BUFFER);
    f.payload()[EM_WRITE_OFFSET_BANK] = bank;
    f.put_u32(EM_WRITE_OFFSET_OFFSET, offset_words);
    f.put_u16(EM_WRITE_OFFSET_DATA_LEN, (words.len() * 2) as u16);
    for (i, w) in words.iter().enumerate() {
        f.put_u16(EM_WRITE_OFFSET_DATA + 2 * i, *w);
    }
    f
}

pub(crate) fn write_pattern_compressed(
    seq: u8,
    bank: u8,
    offset_words: u32,
    format: u8,
    count: u8,
    words: &[u16],
) -> Frame {
    let mut f = Frame::new(seq, CMD_WRITE_PATTERN_COMPRESSED);
    f.payload()[EM_COMPRESSED_OFFSET_BANK] = bank;
    f.payload()[EM_COMPRESSED_OFFSET_FORMAT] = format;
    f.payload()[EM_COMPRESSED_OFFSET_COUNT] = count;
    f.put_u32(EM_COMPRESSED_OFFSET_OFFSET, offset_words);
    for (i, w) in words.iter().enumerate() {
        f.put_u16(EM_COMPRESSED_OFFSET_DATA + 2 * i, *w);
    }
    f
}

pub(crate) fn write_mod_buffer(seq: u8, bank: u8, offset: u32, data: &[u8]) -> Frame {
    let mut f = Frame::new(seq, CMD_WRITE_MOD_BUFFER);
    f.payload()[MOD_WRITE_OFFSET_BANK] = bank;
    f.put_u32(MOD_WRITE_OFFSET_OFFSET, offset);
    f.put_u16(MOD_WRITE_OFFSET_DATA_LEN, data.len() as u16);
    f.payload()[MOD_WRITE_OFFSET_DATA..MOD_WRITE_OFFSET_DATA + data.len()].copy_from_slice(data);
    f
}

pub(crate) fn config_mod(seq: u8, bank: u8, divider: u16, size: u32) -> Frame {
    config_mod_rep(seq, bank, divider, size, REP_INFINITE)
}

pub(crate) fn config_mod_rep(seq: u8, bank: u8, divider: u16, size: u32, rep: u16) -> Frame {
    let mut f = Frame::new(seq, CMD_CONFIG_MOD);
    f.payload()[MOD_CONFIG_OFFSET_BANK] = bank;
    f.put_u16(MOD_CONFIG_OFFSET_DIVIDER, divider);
    f.put_u32(MOD_CONFIG_OFFSET_SIZE, size);
    f.put_u16(MOD_CONFIG_OFFSET_REP, rep);
    f
}

pub(crate) fn config_pattern(
    seq: u8,
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
) -> Frame {
    config_pattern_rep(
        seq,
        bank,
        emission_type,
        divider,
        size,
        num_foci,
        sound_speed,
        REP_INFINITE,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn config_pattern_rep(
    seq: u8,
    bank: u8,
    emission_type: u8,
    divider: u16,
    size: u32,
    num_foci: u8,
    sound_speed: u16,
    rep: u16,
) -> Frame {
    let mut f = Frame::new(seq, CMD_CONFIG_PATTERN);
    f.payload()[EM_CONFIG_OFFSET_BANK] = bank;
    f.payload()[EM_CONFIG_OFFSET_TYPE] = emission_type;
    f.put_u16(EM_CONFIG_OFFSET_DIVIDER, divider);
    f.put_u32(EM_CONFIG_OFFSET_SIZE, size);
    f.payload()[EM_CONFIG_OFFSET_NUM_FOCI] = num_foci;
    f.put_u16(EM_CONFIG_OFFSET_SOUND_SPEED, sound_speed);
    f.put_u16(EM_CONFIG_OFFSET_REP, rep);
    f
}

pub(crate) fn change_pattern_bank(
    seq: u8,
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> Frame {
    let mut f = Frame::new(seq, CMD_CHANGE_PATTERN_BANK);
    f.payload()[CHANGE_BANK_OFFSET_BANK] = bank;
    f.payload()[CHANGE_BANK_OFFSET_TRANSITION_MODE] = transition_mode;
    f.put_u64(CHANGE_BANK_OFFSET_TRANSITION_VALUE, transition_value);
    f
}

pub(crate) fn change_mod_bank(
    seq: u8,
    bank: u8,
    transition_mode: u8,
    transition_value: u64,
) -> Frame {
    let mut f = Frame::new(seq, CMD_CHANGE_MOD_BANK);
    f.payload()[CHANGE_BANK_OFFSET_BANK] = bank;
    f.payload()[CHANGE_BANK_OFFSET_TRANSITION_MODE] = transition_mode;
    f.put_u64(CHANGE_BANK_OFFSET_TRANSITION_VALUE, transition_value);
    f
}

pub(crate) fn set_silencer(
    seq: u8,
    flag: u8,
    update_rate_intensity: u16,
    update_rate_phase: u16,
    completion_steps_intensity: u16,
    completion_steps_phase: u16,
) -> Frame {
    let mut f = Frame::new(seq, CMD_SET_SILENCER);
    f.payload()[SILENCER_OFFSET_FLAG] = flag;
    f.put_u16(SILENCER_OFFSET_UPDATE_RATE_INTENSITY, update_rate_intensity);
    f.put_u16(SILENCER_OFFSET_UPDATE_RATE_PHASE, update_rate_phase);
    f.put_u16(
        SILENCER_OFFSET_COMPLETION_STEPS_INTENSITY,
        completion_steps_intensity,
    );
    f.put_u16(
        SILENCER_OFFSET_COMPLETION_STEPS_PHASE,
        completion_steps_phase,
    );
    f
}

pub(crate) fn force_fan(seq: u8, value: u8) -> Frame {
    let mut f = Frame::new(seq, CMD_FORCE_FAN);
    f.payload()[FORCE_FAN_OFFSET_VALUE] = value;
    f
}

pub(crate) fn gpio_in(seq: u8, flag: u8) -> Frame {
    let mut f = Frame::new(seq, CMD_EMULATE_GPIO_IN);
    f.payload()[GPIO_IN_OFFSET_FLAG] = flag;
    f
}

pub(crate) fn phase_corr(seq: u8, phases: &[u8]) -> Frame {
    let mut f = Frame::new(seq, CMD_SET_PHASE_CORR);
    f.payload()[PHASE_CORR_OFFSET_DATA..PHASE_CORR_OFFSET_DATA + phases.len()]
        .copy_from_slice(phases);
    f
}

pub(crate) fn output_mask(seq: u8, words: &[u16]) -> Frame {
    let mut f = Frame::new(seq, CMD_SET_OUTPUT_MASK);
    for (i, w) in words.iter().enumerate() {
        f.put_u16(OUTPUT_MASK_OFFSET_DATA + 2 * i, *w);
    }
    f
}

pub(crate) fn pwe(seq: u8, table: &[u16]) -> Frame {
    let mut f = Frame::new(seq, CMD_SET_PWE);
    for (i, w) in table.iter().enumerate() {
        f.put_u16(PWE_OFFSET_DATA + 2 * i, *w);
    }
    f
}

pub(crate) fn gpio_out(seq: u8, values: &[u64]) -> Frame {
    let mut f = Frame::new(seq, CMD_SET_GPIO_OUT);
    for (i, v) in values.iter().enumerate() {
        f.put_u64(GPIO_OUT_OFFSET_DATA + 8 * i, *v);
    }
    f
}
