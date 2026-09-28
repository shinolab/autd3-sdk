use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternFusedPayload;

use super::write_pattern_raw::{self, PATTERN_RAW_DATA_LEN};
use crate::app::Cpu;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, CTL_FLAG_PATTERN_SET,
    EMISSION_TYPE_RAW, NUM_BANKS, NUM_TRANSDUCERS,
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
        let data_len = usize::from(p.data_len.get());
        let emission_type = p.emission_type;

        if usize::from(bank) >= NUM_BANKS
            || !data_len.is_multiple_of(2)
            || data_len > PATTERN_FUSED_MAX_DATA_LEN
            || data_len > rest.len()
            || (emission_type == EMISSION_TYPE_RAW
                && (p.size.get() != 1 || data_len != PATTERN_RAW_DATA_LEN))
        {
            return Err(Error::InvalidPayload);
        }

        let cfg = self.validate_pattern_config(
            bank,
            emission_type,
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

        let data = &rest[..data_len];
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
