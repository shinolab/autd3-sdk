use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ConfigModPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{Error, MOD_BUFFER_SAMPLES};

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = ConfigModPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        self.write_mod_config_regs(port, p.bank, p.divider.get(), p.size.get(), p.rep.get())?;
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }

    pub(crate) fn write_mod_config_regs<P: Port>(
        &self,
        port: &mut P,
        bank: u8,
        divider: u16,
        size: u32,
        rep: u16,
    ) -> Result<(), Error> {
        if usize::from(bank) >= NUM_BANKS || divider == 0 || size == 0 || size > MOD_BUFFER_SAMPLES
        {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_mod_div(divider) {
            return Err(Error::InvalidSilencerSetting);
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
        Ok(())
    }
}
