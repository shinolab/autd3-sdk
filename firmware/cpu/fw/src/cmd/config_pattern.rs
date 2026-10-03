pub use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0, BRAM_SELECT_CONTROLLER,
};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn config_pattern<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let cfg = ConfigPatternPayload::parse(payload)?;
        if self.silencer.violates_pattern_div(cfg.divider.get()) {
            return Err(Error::InvalidSilencerSetting);
        }
        self.write_pattern_config(port, &cfg);
        Ok(())
    }

    pub(crate) fn write_pattern_config<P: Port>(&self, port: &mut P, cfg: &ConfigPatternPayload) {
        let bank = cfg.bank.as_u8();
        let divider = cfg.divider.get();
        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_MODE0 + bank_offset,
            cfg.emission_type as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_CYCLE0 + bank_offset,
            (cfg.size.get() - 1) as u16,
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
            cfg.sound_speed.get(),
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_NUM_FOCI0 + bank_offset,
            u16::from(cfg.num_foci),
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + bank_offset,
            cfg.rep.get(),
        );
        self.silencer.pattern_freq_div[usize::from(bank)].set(divider);
    }
}
