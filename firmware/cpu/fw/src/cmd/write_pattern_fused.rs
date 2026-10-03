pub use autd3_cpu_wire::payload::WritePatternFusedPayload;

use super::config_pattern::PatternConfig;
use super::write_pattern_raw;
use crate::app::Cpu;
use crate::cmd::TransitionRequest;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, CTL_FLAG_PATTERN_SET,
    NUM_TRANSDUCERS,
};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn write_pattern_fused<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let (p, data) = WritePatternFusedPayload::parse(payload)?;
        let cfg = PatternConfig {
            bank: p.bank.as_u8(),
            emission_type: p.emission_type,
            divider: p.divider.get(),
            size: p.size.get(),
            num_foci: p.num_foci,
            sound_speed: p.sound_speed.get(),
            rep: p.rep.get(),
        };
        let bank = cfg.bank;
        let change = self.validate_pattern_change(
            port,
            bank,
            cfg.divider,
            cfg.rep,
            &TransitionRequest {
                mode: p.transition_mode,
                value: p.transition_value.get(),
                margin_ns: p.margin_ns.get(),
            },
        )?;

        if let Some((phases, intensities)) = data.split_first_chunk::<NUM_TRANSDUCERS>()
            && let Ok(intensities) = <&[u8; NUM_TRANSDUCERS]>::try_from(intensities)
            && cfg.emission_type == EmissionType::Raw
        {
            write_pattern_raw::write_slot(port, bank, 0, phases, intensities);
        } else {
            fpga::write_ram(
                port,
                BRAM_SELECT_EMISSION,
                ADDR_PATTERN_MEM_WR_BANK,
                ADDR_PATTERN_MEM_WR_PAGE,
                bank,
                0,
                data,
            );
        }
        self.write_pattern_config(port, &cfg);
        self.write_pattern_change(port, &change);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
