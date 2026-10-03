pub use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::app::Cpu;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET,
};
use crate::port::Port;
use crate::proto::Error;

pub(crate) struct PatternConfig {
    pub(crate) bank: u8,
    pub(crate) emission_type: EmissionType,
    pub(crate) divider: u16,
    pub(crate) size: u32,
    pub(crate) num_foci: u8,
    pub(crate) sound_speed: u16,
    pub(crate) rep: u16,
}

impl Cpu {
    pub(crate) fn config_pattern<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let p = ConfigPatternPayload::parse(payload)?;
        let cfg = PatternConfig {
            bank: p.bank.as_u8(),
            emission_type: p.emission_type,
            divider: p.divider.get(),
            size: p.size.get(),
            num_foci: p.num_foci,
            sound_speed: p.sound_speed.get(),
            rep: p.rep.get(),
        };
        if self.silencer.violates_pattern_div(cfg.divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        self.write_pattern_config(port, &cfg);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }

    pub(crate) fn write_pattern_config<P: Port>(&self, port: &mut P, cfg: &PatternConfig) {
        let bank_offset = u16::from(cfg.bank);
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
            (cfg.size - 1) as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_FREQ_DIV0 + bank_offset,
            cfg.divider,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_SOUND_SPEED0 + bank_offset,
            cfg.sound_speed,
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
            cfg.rep,
        );
        self.silencer.pattern_freq_div[usize::from(cfg.bank)].set(cfg.divider);
    }
}
