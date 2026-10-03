pub use autd3_cpu_wire::payload::ConfigModPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
};
use crate::port::Port;
use crate::proto::Error;

pub(crate) struct ModConfig {
    pub(crate) bank: u8,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) rep: u16,
}

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let p = ConfigModPayload::parse(payload)?;
        let cfg = ModConfig {
            bank: p.bank.as_u8(),
            divider: p.divider.get(),
            size: p.size.get(),
            rep: p.rep.get(),
        };
        if self.silencer.violates_mod_div(cfg.divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        self.write_mod_config(port, &cfg);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }

    pub(crate) fn write_mod_config<P: Port>(&self, port: &mut P, cfg: &ModConfig) {
        let bank_offset = u16::from(cfg.bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_CYCLE0 + bank_offset,
            (cfg.size - 1) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_FREQ_DIV0 + bank_offset,
            cfg.divider,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + bank_offset,
            cfg.rep,
        );
        self.silencer.mod_freq_div[usize::from(cfg.bank)].set(cfg.divider);
    }
}
