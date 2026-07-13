use crate::app::Cpu;
use crate::fpga::{self, SYS_TIME_TRANSITION_MARGIN_NS, transition_mode_violates_loop};
use crate::params::{
    ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK, ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0,
    BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET, NUM_BANKS, TRANSITION_MODE_SYS_TIME,
};
use crate::port::Port;
use crate::proto::{
    CHANGE_BANK_OFFSET_BANK, CHANGE_BANK_OFFSET_TRANSITION_MODE,
    CHANGE_BANK_OFFSET_TRANSITION_VALUE, ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING,
    ERR_INVALID_TRANSITION_MODE, ERR_MISS_TRANSITION_TIME, read_u64,
};

impl Cpu {
    pub(crate) fn change_mod_bank<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let bank = payload[CHANGE_BANK_OFFSET_BANK];
        let transition_mode = payload[CHANGE_BANK_OFFSET_TRANSITION_MODE];
        let transition_value = read_u64(payload, CHANGE_BANK_OFFSET_TRANSITION_VALUE);

        if usize::from(bank) >= NUM_BANKS {
            return ERR_INVALID_PAYLOAD;
        }
        if self.silencer.violates_mod_bank(bank) {
            return ERR_INVALID_SILENCER_SETTING;
        }
        let rep = fpga::read(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + u16::from(bank),
        );
        if transition_mode_violates_loop(rep, transition_mode) {
            return ERR_INVALID_TRANSITION_MODE;
        }
        if transition_mode == TRANSITION_MODE_SYS_TIME
            && transition_value < port.dc_sys_time() + SYS_TIME_TRANSITION_MARGIN_NS
        {
            return ERR_MISS_TRANSITION_TIME;
        }

        fpga::write_change_bank(
            port,
            ADDR_MOD_REQ_RD_BANK,
            ADDR_MOD_TRANSITION_MODE,
            ADDR_MOD_TRANSITION_VALUE_0,
            bank,
            transition_mode,
            transition_value,
        );
        self.silencer.note_mod_bank(bank);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }
}
