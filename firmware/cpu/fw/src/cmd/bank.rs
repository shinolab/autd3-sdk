use core::num::NonZeroU32;

use crate::fpga::{self, TransitionModeKind, transition_register_value};
use crate::fpga_params::{
    ADDR_MOD_REQ_RD_BANK, ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0,
    ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0,
    CtlFlags,
};
use crate::port::Port;
use crate::proto::Error;

pub(crate) struct BankRegs {
    req_rd_bank: u16,
    transition_mode: u16,
    transition_value: u16,
    set_flag: CtlFlags,
}

pub(crate) const MOD_BANK_REGS: BankRegs = BankRegs {
    req_rd_bank: ADDR_MOD_REQ_RD_BANK,
    transition_mode: ADDR_MOD_TRANSITION_MODE,
    transition_value: ADDR_MOD_TRANSITION_VALUE_0,
    set_flag: CtlFlags::MOD_SET,
};

pub(crate) const PATTERN_BANK_REGS: BankRegs = BankRegs {
    req_rd_bank: ADDR_PATTERN_REQ_RD_BANK,
    transition_mode: ADDR_PATTERN_TRANSITION_MODE,
    transition_value: ADDR_PATTERN_TRANSITION_VALUE_0,
    set_flag: CtlFlags::PATTERN_SET,
};

pub(crate) fn activate_bank<P: Port>(
    port: &mut P,
    regs: &BankRegs,
    bank: u8,
    mode: TransitionModeKind,
    value: u64,
    max_polls: NonZeroU32,
) -> Result<(), Error> {
    fpga::write_ctl(port, regs.transition_mode, mode as u16);
    fpga::write_u64(
        port,
        regs.transition_value,
        transition_register_value(mode, value),
    );
    fpga::write_ctl(port, regs.req_rd_bank, u16::from(bank));
    fpga::set_and_wait_update(port, regs.set_flag, max_polls)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use core::mem::offset_of;

    use crate::cmd::activate_mod_bank::ActivateModBankPayload;
    use crate::cmd::activate_pattern_bank::ActivatePatternBankPayload;
    use crate::fpga::TransitionModeKind;
    use crate::fpga_params::{ADDR_MOD_REQ_RD_BANK, ADDR_PATTERN_REQ_RD_BANK};
    use crate::proto::Error;
    use crate::test_utils::builders::{activate_mod_bank, activate_pattern_bank};
    use crate::test_utils::mock::Harness;

    #[test]
    fn activate_bank_rejects_unknown_transition_mode_as_invalid_payload() {
        let mut h = Harness::new();
        let unknown = 0x03;
        assert_eq!(TransitionModeKind::from_u8(unknown), None);

        let mut frame = activate_mod_bank(1, TransitionModeKind::Ext, 0);
        frame.set_payload_byte(offset_of!(ActivateModBankPayload, transition_mode), unknown);
        h.deliver(&frame);
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

        let mut frame = activate_pattern_bank(1, TransitionModeKind::Immediate, 0);
        frame.set_payload_byte(
            offset_of!(ActivatePatternBankPayload, transition_mode),
            unknown,
        );
        h.deliver(&frame);
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
    }
}
