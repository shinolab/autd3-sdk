use zerocopy::FromBytes;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{
    ConfigModPayload, ERR_INVALID_PAYLOAD, ERR_INVALID_SILENCER_SETTING, MOD_BUFFER_SAMPLES,
};

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> u8 {
        let Ok((p, _)) = ConfigModPayload::ref_from_prefix(payload) else {
            return ERR_INVALID_PAYLOAD;
        };
        let bank = p.bank;
        let divider = p.divider.get();
        let size = p.size.get();
        let rep = p.rep.get();

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
