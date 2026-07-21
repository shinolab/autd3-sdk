use zerocopy::FromBytes;

pub use autd3_cpu_wire::layout::MOD_FUSED_MAX_DATA_LEN;
pub use autd3_cpu_wire::payload::WriteModulationFusedPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::params::{
    ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, CTL_FLAG_MOD_SET, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::{Error, MOD_BUFFER_SAMPLES};

impl Cpu {
    pub(crate) fn write_mod_fused<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let Ok((p, rest)) = WriteModulationFusedPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        let data_len = p.data_len.get();

        if usize::from(bank) >= NUM_BANKS
            || usize::from(data_len) > MOD_FUSED_MAX_DATA_LEN
            || usize::from(data_len) > rest.len()
            || u32::from(data_len) > MOD_BUFFER_SAMPLES
        {
            return Err(Error::InvalidPayload);
        }

        fpga::write_ram(
            port,
            BRAM_SELECT_MOD,
            ADDR_MOD_MEM_WR_BANK,
            ADDR_MOD_MEM_WR_PAGE,
            bank,
            0,
            &rest[..usize::from(data_len)],
        );
        self.write_mod_config_regs(port, bank, p.divider.get(), p.size.get(), p.rep.get())?;
        self.write_mod_change_regs(
            port,
            bank,
            p.transition_mode,
            p.transition_value.get(),
            p.margin_ns.get(),
        )?;
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }
}
