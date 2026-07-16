use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::app::Cpu;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET,
    EMISSION_MAX_INDICES, NUM_BANKS, NUM_FOCI_MAX,
};
use crate::port::Port;
use crate::proto::{Error, MAX_FOCI_TOTAL};

impl Cpu {
    pub(crate) fn config_pattern<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let Ok((p, _)) = ConfigPatternPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        let divider = p.divider.get();
        let size = p.size.get();
        let num_foci = p.num_foci;
        let sound_speed = p.sound_speed.get();
        let rep = p.rep.get();

        let Some(emission_type) = EmissionType::from_u8(p.emission_type) else {
            return Err(Error::InvalidPayload);
        };
        let mut invalid = usize::from(bank) >= NUM_BANKS || divider == 0 || size == 0;
        if !invalid {
            invalid = match emission_type {
                EmissionType::Raw => size > EMISSION_MAX_INDICES,
                EmissionType::Foci => {
                    num_foci == 0
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

        let bank_offset = u16::from(bank);
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_MODE0 + bank_offset,
            emission_type as u16,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_CYCLE0 + bank_offset,
            (size - 1) as u16,
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
            sound_speed,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_NUM_FOCI0 + bank_offset,
            u16::from(num_foci),
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + bank_offset,
            rep,
        );
        self.silencer.note_pattern_div(bank, divider);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
