pub use autd3_cpu_wire::payload::WriteModulationFusedPayload;

use super::config_mod::ModConfig;
use crate::app::Cpu;
use crate::cmd::TransitionRequest;
use crate::fpga;
use crate::params::{
    ADDR_MOD_MEM_WR_BANK, ADDR_MOD_MEM_WR_PAGE, BRAM_SELECT_MOD, CTL_FLAG_MOD_SET,
};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn write_mod_fused<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let (p, data) = WriteModulationFusedPayload::parse(payload)?;
        let cfg = ModConfig {
            bank: p.bank.as_u8(),
            divider: p.divider.get(),
            size: p.size.get(),
            rep: p.rep.get(),
        };
        let change = self.validate_mod_change(
            port,
            cfg.bank,
            cfg.divider,
            cfg.rep,
            &TransitionRequest {
                mode: p.transition_mode,
                value: p.transition_value.get(),
                margin_ns: p.margin_ns.get(),
            },
        )?;

        fpga::write_ram(
            port,
            BRAM_SELECT_MOD,
            ADDR_MOD_MEM_WR_BANK,
            ADDR_MOD_MEM_WR_PAGE,
            cfg.bank,
            0,
            data,
        );
        self.write_mod_config(port, &cfg);
        self.write_mod_change(port, &change);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }
}
