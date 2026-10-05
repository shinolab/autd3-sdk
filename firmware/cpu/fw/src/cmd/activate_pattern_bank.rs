pub use autd3_cpu_wire::payload::ActivatePatternBankPayload;

use crate::cmd::bank::{PATTERN_BANK_REGS, activate_bank};
use crate::cmd::cpu_config::CpuConfig;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(
    port: &mut P,
    config: &CpuConfig,
    payload: &[u8],
) -> Result<(), Error> {
    let p = ActivatePatternBankPayload::parse(payload)?;
    activate_bank(
        port,
        &PATTERN_BANK_REGS,
        p.bank.as_u8(),
        p.transition_mode,
        p.transition_value.get(),
        config,
    )
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga::TransitionMode;
    use crate::fpga_params::{
        ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, ADDR_PATTERN_TRANSITION_VALUE_0,
        CtlFlags, EMISSION_MAX_INDICES, EmissionType,
    };
    use crate::proto::Error;
    use crate::test_utils::builders::{activate_pattern_bank, config_pattern_rep, invalid_bank};
    use crate::test_utils::mock::Harness;

    const SYS_TIME_TRANSITION_MARGIN_NS: u64 =
        autd3_cpu_wire::cpu_params::SYS_TIME_TRANSITION_MARGIN.as_nanos() as u64;

    #[test]
    fn activate_pattern_bank_writes_transition_and_req_bank_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::PATTERN_SET);

        h.deliver(&activate_pattern_bank(0, 1, TransitionMode::Immediate, 0));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_TRANSITION_MODE),
            TransitionMode::Immediate as u16
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
            1,
            0,
            TransitionMode::SysTime,
            0x0123_4567_89AB_CDEF,
        ));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_TRANSITION_MODE),
            TransitionMode::SysTime as u16
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
            0,
            invalid_bank(),
            TransitionMode::Immediate,
            0,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
    }

    #[test]
    fn activate_pattern_bank_rejects_timed_transition_on_infinite_loop() {
        let mut h = Harness::new();

        h.deliver(&activate_pattern_bank(0, 1, TransitionMode::Gpio, 0));
        assert_eq!(h.status(), Error::InvalidTransitionMode);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

        h.deliver(&activate_pattern_bank(1, 1, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    }

    #[test]
    fn activate_pattern_bank_rejects_immediate_transition_on_finite_loop() {
        let mut h = Harness::new();
        h.deliver(&config_pattern_rep(
            0,
            1,
            EmissionType::Raw.as_u8(),
            2,
            EMISSION_MAX_INDICES,
            0,
            0,
            4,
        ));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_pattern_bank(1, 1, TransitionMode::Ext, 0));
        assert_eq!(h.status(), Error::InvalidTransitionMode);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

        h.deliver(&activate_pattern_bank(2, 1, TransitionMode::SyncIdx, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    }

    #[test]
    fn activate_pattern_bank_rejects_sys_time_transition_within_margin() {
        let mut h = Harness::new();
        h.deliver(&config_pattern_rep(
            0,
            1,
            EmissionType::Raw.as_u8(),
            2,
            EMISSION_MAX_INDICES,
            0,
            0,
            4,
        ));
        assert_eq!(h.status(), Error::None);
        h.port.sys_time = Some(2_000_000_000);

        h.deliver(&activate_pattern_bank(
            1,
            1,
            TransitionMode::SysTime,
            2_000_000_000,
        ));
        assert_eq!(h.status(), Error::MissTransitionTime);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);

        h.deliver(&activate_pattern_bank(
            2,
            1,
            TransitionMode::SysTime,
            2_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS + 1,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    }
}
