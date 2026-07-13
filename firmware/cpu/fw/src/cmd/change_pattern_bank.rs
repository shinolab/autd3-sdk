use zerocopy::FromBytes;

use crate::app::Cpu;
use crate::fpga::{self, sys_time_margin_ns, transition_mode_violates_loop};
use crate::params::{
    ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET, NUM_BANKS,
    TRANSITION_MODE_SYS_TIME,
};
use crate::port::Port;
use crate::proto::{
    ChangeBankPayload, ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING,
    ERR_INVALID_TRANSITION_MODE, ERR_MISS_TRANSITION_TIME,
};

impl Cpu {
    pub(crate) fn change_pattern_bank<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let Ok((p, _)) = ChangeBankPayload::ref_from_prefix(payload) else {
            return ERR_INVALID_PAYLOAD;
        };
        let bank = p.bank;
        let transition_mode = p.transition_mode;
        let transition_value = p.transition_value.get();
        let margin_ns = sys_time_margin_ns(p.margin_ns.get());

        if usize::from(bank) >= NUM_BANKS {
            return ERR_INVALID_PAYLOAD;
        }
        if self.silencer.violates_pattern_bank(bank) {
            return ERR_INVALID_SILENCER_SETTING;
        }
        let rep = fpga::read(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + u16::from(bank),
        );
        if transition_mode_violates_loop(rep, transition_mode) {
            return ERR_INVALID_TRANSITION_MODE;
        }
        if transition_mode == TRANSITION_MODE_SYS_TIME
            && transition_value < port.dc_sys_time() + margin_ns
        {
            return ERR_MISS_TRANSITION_TIME;
        }

        fpga::write_change_bank(
            port,
            ADDR_PATTERN_REQ_RD_BANK,
            ADDR_PATTERN_TRANSITION_MODE,
            ADDR_PATTERN_TRANSITION_VALUE_0,
            bank,
            transition_mode,
            transition_value,
        );
        self.silencer.note_pattern_bank(bank);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
