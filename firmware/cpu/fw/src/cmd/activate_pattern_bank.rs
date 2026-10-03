pub use autd3_cpu_wire::payload::ActivatePatternBankPayload;

use crate::app::Cpu;
use crate::cmd::{BankActivation, TransitionRequest};
use crate::fpga::{self, transition_register_value, validate_transition_mode};
use crate::params::{
    ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE,
    ADDR_PATTERN_TRANSITION_VALUE_0, BRAM_SELECT_CONTROLLER, CTL_FLAG_PATTERN_SET,
};
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn activate_pattern_bank<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let p = ActivatePatternBankPayload::parse(payload)?;
        let bank = p.bank.as_u8();
        let rep = fpga::read(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REP0 + u16::from(bank),
        );
        let change = self.validate_pattern_activation(
            port,
            bank,
            self.silencer.pattern_freq_div[usize::from(bank)].get(),
            rep,
            &TransitionRequest {
                mode: p.transition_mode,
                value: p.transition_value.get(),
                margin_ns: p.margin_ns.get(),
            },
        )?;
        self.write_pattern_activation(port, &change);
        self.set_and_wait_update(port, CTL_FLAG_PATTERN_SET)
    }

    pub(crate) fn validate_pattern_activation<P: Port>(
        &self,
        port: &mut P,
        bank: u8,
        divider: u16,
        rep: u16,
        transition: &TransitionRequest,
    ) -> Result<BankActivation, Error> {
        if self.silencer.violates_pattern_div(divider) {
            return Err(Error::InvalidSilencerSetting);
        }
        validate_transition_mode(
            port,
            rep,
            transition.mode,
            transition.value,
            transition.margin_ns(),
        )?;
        Ok(BankActivation {
            bank,
            transition_mode: transition.mode,
            transition_value: transition_register_value(transition.mode, transition.value),
        })
    }

    pub(crate) fn write_pattern_activation<P: Port>(&self, port: &mut P, change: &BankActivation) {
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_TRANSITION_MODE,
            change.transition_mode as u16,
        );
        fpga::write_u64(
            port,
            ADDR_PATTERN_TRANSITION_VALUE_0,
            change.transition_value,
        );
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            ADDR_PATTERN_REQ_RD_BANK,
            u16::from(change.bank),
        );
        self.silencer.pattern_bank.set(change.bank);
        self.silencer
            .pattern_latched_div
            .set(self.silencer.pattern_freq_div[usize::from(change.bank)].get());
    }
}
