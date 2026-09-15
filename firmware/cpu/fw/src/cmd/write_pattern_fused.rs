use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternFusedPayload;

use crate::app::Cpu;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, CTL_FLAG_PATTERN_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{Error, PAYLOAD_BYTES};

const PATTERN_FUSED_MAX_DATA_LEN: usize =
    PAYLOAD_BYTES - core::mem::size_of::<WritePatternFusedPayload>();

impl Cpu {
    pub(crate) fn write_pattern_fused<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let Ok((p, rest)) = WritePatternFusedPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        let data_len = p.data_len.get();

        if usize::from(bank) >= NUM_BANKS
            || !data_len.is_multiple_of(2)
            || usize::from(data_len) > PATTERN_FUSED_MAX_DATA_LEN
            || usize::from(data_len) > rest.len()
            || (matches!(
                EmissionType::from_u8(p.emission_type),
                Some(EmissionType::Raw)
            ) && p.size.get() != 1)
        {
            return Err(Error::InvalidPayload);
        }

        let cfg = self.validate_pattern_config(
            bank,
            p.emission_type,
            p.divider.get(),
            p.size.get(),
            p.num_foci,
            p.sound_speed.get(),
            p.rep.get(),
        )?;
        let change = self.validate_pattern_change(
            port,
            bank,
            cfg.divider,
            cfg.rep,
            p.transition_mode,
            p.transition_value.get(),
            p.margin_ns.get(),
        )?;

        fpga::write_ram(
            port,
            BRAM_SELECT_EMISSION,
            ADDR_PATTERN_MEM_WR_BANK,
            ADDR_PATTERN_MEM_WR_PAGE,
            bank,
            0,
            &rest[..usize::from(data_len)],
        );
        self.write_pattern_config(port, &cfg);
        self.write_pattern_change(port, &change);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
