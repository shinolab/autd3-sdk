use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET,
    EMISSION_MAX_INDICES, EMISSION_TYPE_RAW, NUM_BANKS, NUM_FOCI_MAX,
};
use crate::port::Port;
use crate::proto::{
    EM_CONFIG_OFFSET_BANK, EM_CONFIG_OFFSET_DIVIDER, EM_CONFIG_OFFSET_NUM_FOCI,
    EM_CONFIG_OFFSET_REP, EM_CONFIG_OFFSET_SIZE, EM_CONFIG_OFFSET_SOUND_SPEED,
    EM_CONFIG_OFFSET_TYPE, ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING, MAX_FOCI_TOTAL,
    read_u16, read_u32,
};

impl Cpu {
    pub(crate) fn config_pattern<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let bank = payload[EM_CONFIG_OFFSET_BANK];
        let emission_type = payload[EM_CONFIG_OFFSET_TYPE];
        let divider = read_u16(payload, EM_CONFIG_OFFSET_DIVIDER);
        let size = read_u32(payload, EM_CONFIG_OFFSET_SIZE);
        let num_foci = payload[EM_CONFIG_OFFSET_NUM_FOCI];
        let sound_speed = read_u16(payload, EM_CONFIG_OFFSET_SOUND_SPEED);
        let rep = read_u16(payload, EM_CONFIG_OFFSET_REP);

        let mut invalid = usize::from(bank) >= NUM_BANKS
            || emission_type > EMISSION_TYPE_RAW
            || divider == 0
            || size == 0;
        if !invalid {
            invalid = if emission_type == EMISSION_TYPE_RAW {
                size > EMISSION_MAX_INDICES
            } else {
                num_foci == 0
                    || num_foci > NUM_FOCI_MAX
                    || size > MAX_FOCI_TOTAL / u32::from(num_foci)
                    || sound_speed == 0
            };
        }
        if invalid {
            return ERR_INVALID_PAYLOAD;
        }
        if self.silencer.violates_pattern_div(divider) {
            return ERR_INVALID_SILENCER_SETTING;
        }

        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_MODE0 + bank_offset,
            u16::from(emission_type),
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_CYCLE0 + bank_offset,
            (size - 1) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_FREQ_DIV0 + bank_offset,
            divider,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_SOUND_SPEED0 + bank_offset,
            sound_speed,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_NUM_FOCI0 + bank_offset,
            u16::from(num_foci),
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + bank_offset,
            rep,
        );
        self.silencer.note_pattern_div(bank, divider);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
