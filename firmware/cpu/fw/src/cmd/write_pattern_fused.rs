use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternFusedPayload;

use super::write_pattern_raw::{self, PATTERN_RAW_DATA_LEN};
use crate::app::Cpu;
use crate::fpga::{self, EmissionType};
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, CTL_FLAG_PATTERN_SET,
    EMISSION_TYPE_FOCI, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{Error, PAYLOAD_BYTES};
use autd3_cpu_wire::layout::FUSED_EMISSION_TYPE_RAW_SOA;

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
        let emission_type = match p.emission_type {
            FUSED_EMISSION_TYPE_RAW_SOA => EmissionType::Raw as u8,
            EMISSION_TYPE_FOCI => EMISSION_TYPE_FOCI,
            _ => return Err(Error::InvalidPayload),
        };

        if usize::from(bank) >= NUM_BANKS
            || !data_len.is_multiple_of(2)
            || usize::from(data_len) > PATTERN_FUSED_MAX_DATA_LEN
            || usize::from(data_len) > rest.len()
            || (p.emission_type == FUSED_EMISSION_TYPE_RAW_SOA
                && (p.size.get() != 1 || usize::from(data_len) != PATTERN_RAW_DATA_LEN))
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

        let data = &rest[..usize::from(data_len)];
        if let Ok(raw) = <&[u8; PATTERN_RAW_DATA_LEN]>::try_from(data)
            && cfg.emission_type == EmissionType::Raw
        {
            write_pattern_raw::write_slot(port, bank, 0, raw);
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
