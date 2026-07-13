use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{
    ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING, MOD_BUFFER_SAMPLES, MOD_CONFIG_OFFSET_BANK,
    MOD_CONFIG_OFFSET_DIVIDER, MOD_CONFIG_OFFSET_REP, MOD_CONFIG_OFFSET_SIZE, read_u16, read_u32,
};

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let bank = payload[MOD_CONFIG_OFFSET_BANK];
        let divider = read_u16(payload, MOD_CONFIG_OFFSET_DIVIDER);
        let size = read_u32(payload, MOD_CONFIG_OFFSET_SIZE);
        let rep = read_u16(payload, MOD_CONFIG_OFFSET_REP);

        if usize::from(bank) >= NUM_BANKS || divider == 0 || size == 0 || size > MOD_BUFFER_SAMPLES
        {
            return ERR_INVALID_PAYLOAD;
        }
        if self.silencer.violates_mod_div(divider) {
            return ERR_INVALID_SILENCER_SETTING;
        }

        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_CYCLE0 + bank_offset,
            (size - 1) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_FREQ_DIV0 + bank_offset,
            divider,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + bank_offset,
            rep,
        );
        self.silencer.note_mod_div(bank, divider);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }
}
