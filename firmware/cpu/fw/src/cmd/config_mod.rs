use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ConfigModPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{BUFFER_SIZE_MIN, Error, MOD_BUFFER_SAMPLES};

pub(crate) struct ModConfig {
    pub(crate) bank: u8,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) rep: u16,
}

impl Cpu {
    pub(crate) fn config_mod<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = ConfigModPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let cfg = self.validate_mod_config(p.bank, p.divider.get(), p.size.get(), p.rep.get())?;
        self.write_mod_config(port, &cfg);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }

    pub(crate) fn validate_mod_config(
        &self,
        bank: u8,
        divider: u16,
        size: u32,
        rep: u16,
    ) -> Result<ModConfig, Error> {
        if usize::from(bank) >= NUM_BANKS
            || divider == 0
            || !(BUFFER_SIZE_MIN..=MOD_BUFFER_SAMPLES).contains(&size)
        {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_mod_div(divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        Ok(ModConfig {
            bank,
            divider,
            size,
            rep,
        })
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
        self.silencer.note_mod_div(cfg.bank, cfg.divider);
    }
}
