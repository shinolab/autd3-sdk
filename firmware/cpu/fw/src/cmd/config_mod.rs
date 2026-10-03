pub use autd3_cpu_wire::payload::ConfigModPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let cfg = ConfigModPayload::parse(payload)?;
        if self.silencer.violates_mod_div(cfg.divider.get()) {
            return Err(Error::InvalidSilencerSetting);
        }
        self.write_mod_config(port, &cfg);
        Ok(())
    }

    pub(crate) fn write_mod_config<P: Port>(&self, port: &mut P, cfg: &ConfigModPayload) {
        let bank = cfg.bank.as_u8();
        let divider = cfg.divider.get();
        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_CYCLE0 + bank_offset,
            (cfg.size.get() - 1) as u16,
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
            cfg.rep.get(),
        );
        self.silencer.mod_freq_div[usize::from(bank)].set(divider);
    }
}
