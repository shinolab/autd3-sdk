use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::WritePatternFusedPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_PATTERN_MEM_WR_BANK, ADDR_PATTERN_MEM_WR_PAGE, BRAM_SELECT_EMISSION, CTL_FLAG_PATTERN_SET,
    NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{EMISSION_RAM_WORDS, Error, PAYLOAD_BYTES};

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
            || u32::from(data_len / 2) > EMISSION_RAM_WORDS
        {
            return Err(Error::InvalidPayload);
        }

        fpga::write_ram(
            port,
            BRAM_SELECT_EMISSION,
            ADDR_PATTERN_MEM_WR_BANK,
            ADDR_PATTERN_MEM_WR_PAGE,
            bank,
            0,
            &rest[..usize::from(data_len)],
        );
        self.write_pattern_config_regs(
            port,
            bank,
            p.emission_type,
            p.divider.get(),
            p.size.get(),
            p.num_foci,
            p.sound_speed.get(),
            p.rep.get(),
        )?;
        self.write_pattern_change_regs(
            port,
            bank,
            p.transition_mode,
            p.transition_value.get(),
            p.margin_ns.get(),
        )?;
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }
}
