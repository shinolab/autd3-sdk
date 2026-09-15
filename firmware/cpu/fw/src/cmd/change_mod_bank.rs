use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::ChangeModBankPayload;

use crate::app::Cpu;
use crate::cmd::BankChange;
use crate::fpga::{self, TransitionMode, sys_time_margin_ns, transition_mode_violates_loop};
use crate::params::{
    ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK, ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0,
    BRAM_SELECT_CONTROLLER, CTL_FLAG_MOD_SET, NUM_BANKS,
};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn change_mod_bank<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let Ok((p, _)) = ChangeModBankPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let bank = p.bank;
        if usize::from(bank) >= NUM_BANKS {
            return Err(Error::InvalidPayload);
        }
        let rep = fpga::read(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_MOD_REP0 + u16::from(bank),
        );
        let change = self.validate_mod_change(
            port,
            bank,
            self.silencer.mod_div(bank),
            rep,
            p.transition_mode,
            p.transition_value.get(),
            p.margin_ns.get(),
        )?;
        self.write_mod_change(port, &change);
        self.set_and_wait_update(port, CTL_FLAG_MOD_SET)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn validate_mod_change<P: Port>(
        &self,
        port: &mut P,
        bank: u8,
        divider: u16,
        rep: u16,
        transition_mode: u8,
        transition_value: u64,
        margin_ns_raw: u32,
    ) -> Result<BankChange, Error> {
        let margin_ns = sys_time_margin_ns(margin_ns_raw);

        if usize::from(bank) >= NUM_BANKS {
            return Err(Error::InvalidPayload);
        }
        if self.silencer.violates_mod_div(divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        let Some(transition_mode) = TransitionMode::from_u8(transition_mode) else {
            return Err(Error::InvalidTransitionMode);
        };
        if transition_mode_violates_loop(rep, transition_mode) {
            return Err(Error::InvalidTransitionMode);
        }
        if transition_mode == TransitionMode::SysTime
            && transition_value < port.dc_sys_time() + margin_ns
        {
            return Err(Error::MissTransitionTime);
        }
        Ok(BankChange {
            bank,
            transition_mode,
            transition_value,
        })
    }

    pub(crate) fn write_mod_change<P: Port>(&self, port: &mut P, change: &BankChange) {
        fpga::write_change_bank(
            port,
            ADDR_MOD_REQ_RD_BANK,
            ADDR_MOD_TRANSITION_MODE,
            ADDR_MOD_TRANSITION_VALUE_0,
            change.bank,
            change.transition_mode,
            change.transition_value,
        );
        self.silencer.note_mod_bank(change.bank);
    }
}
