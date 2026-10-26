pub(crate) use autd3_cpu_wire::payload::ActivatePatternBankPayload;

use crate::cmd::bank::{PATTERN_BANK_REGS, activate_bank};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = ActivatePatternBankPayload::parse(payload)?;
    activate_bank(
        port,
        &PATTERN_BANK_REGS,
        p.bank.as_u8(),
        p.transition_mode,
        p.transition_value.get(),
    )
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga::TransitionModeKind;
    use crate::fpga_params::{
        ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0,
        CtlFlags, EMISSION_MAX_INDICES, EmissionType,
    };
    use crate::proto::Error;
    use crate::test_utils::builders::{activate_pattern_bank, config_pattern_rep, invalid_bank};
    use crate::test_utils::mock::Harness;

    #[test]
    fn activate_pattern_bank_writes_transition_and_req_bank_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::PATTERN_SET);

        h.deliver(&activate_pattern_bank(1, TransitionModeKind::Immediate, 0));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_TRANSITION_MODE),
            TransitionModeKind::Immediate as u16
        );
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
        assert_eq!(h.latch_count(CtlFlags::PATTERN_SET), latches_at_boot + 1);
        assert!(!h.ctl_flags().contains(CtlFlags::PATTERN_SET));
    }

    #[test]
    fn activate_pattern_bank_writes_sys_time_transition_in_sys_time_ticks() {
        let mut h = Harness::new();

        h.deliver(&config_pattern_rep(
            0,
            EmissionType::Raw.as_u8(),
            2,
            EMISSION_MAX_INDICES,
            0,
            0,
            4,
        ));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_pattern_bank(
            0,
            TransitionModeKind::SysTime,
            0x0123_4567_89AB_CDEF,
        ));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_TRANSITION_MODE),
            TransitionModeKind::SysTime as u16
        );
        assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0), 0x26C0);
        assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 1), 0x77B8);
        assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 2), 0xF719);
        assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_VALUE_0 + 3), 0x0005);
    }

    #[test]
    fn activate_pattern_bank_rejects_invalid_bank() {
        let mut h = Harness::new();
        h.deliver(&activate_pattern_bank(
            invalid_bank(),
            TransitionModeKind::Immediate,
            0,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
    }

    #[test]
    fn activate_pattern_bank_leaves_the_transition_and_the_loop_to_the_host() {
        let mut h = Harness::new();
        h.port.sys_time = Some(2_000_000_000);

        h.deliver(&activate_pattern_bank(1, TransitionModeKind::Gpio, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);

        h.deliver(&config_pattern_rep(
            0,
            EmissionType::Raw.as_u8(),
            2,
            EMISSION_MAX_INDICES,
            0,
            0,
            4,
        ));
        h.deliver(&activate_pattern_bank(0, TransitionModeKind::Ext, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

        h.deliver(&activate_pattern_bank(
            1,
            TransitionModeKind::SysTime,
            2_000_000_000,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    }
}
