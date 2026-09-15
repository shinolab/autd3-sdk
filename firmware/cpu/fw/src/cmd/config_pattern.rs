use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::app::Cpu;
use crate::fpga::{self, EmissionType, REP_INFINITE};
use crate::params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET,
    EMISSION_MAX_INDICES, NUM_BANKS, NUM_FOCI_MAX,
};
use crate::port::Port;
use crate::proto::{BUFFER_SIZE_MIN, Error, MAX_FOCI_TOTAL};

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
        let Ok((p, _)) = ConfigPatternPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let cfg = self.validate_pattern_config(
            p.bank,
            p.emission_type,
            p.divider.get(),
            p.size.get(),
            p.num_foci,
            p.sound_speed.get(),
            p.rep.get(),
        )?;
        self.write_pattern_config(port, &cfg);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_pattern_config(
        &self,
        bank: u8,
        emission_type: u8,
        divider: u16,
        size: u32,
        num_foci: u8,
        sound_speed: u16,
        rep: u16,
    ) -> Result<PatternConfig, Error> {
        let Some(emission_type) = EmissionType::from_u8(emission_type) else {
            return Err(Error::InvalidPayload);
        };
        let mut invalid = usize::from(bank) >= NUM_BANKS
            || divider == 0
            || size == 0
            || (size < BUFFER_SIZE_MIN && rep != REP_INFINITE);
        if !invalid {
            invalid = match emission_type {
                EmissionType::Raw => size > EMISSION_MAX_INDICES,
                EmissionType::Foci => {
                    size < BUFFER_SIZE_MIN
                        || num_foci == 0
                        || num_foci > NUM_FOCI_MAX
                        || size > MAX_FOCI_TOTAL / u32::from(num_foci)
                        || sound_speed == 0
                }
            };
        }
        if invalid {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_pattern_div(divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        Ok(PatternConfig {
            bank,
            emission_type,
            divider,
            size,
            num_foci,
            sound_speed,
            rep,
        })
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
