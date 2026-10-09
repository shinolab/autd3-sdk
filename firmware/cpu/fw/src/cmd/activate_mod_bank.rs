pub use autd3_cpu_wire::payload::ActivateModBankPayload;

use crate::cmd::bank::{MOD_BANK_REGS, activate_bank};
use crate::cmd::cpu_config::CpuConfig;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(
    port: &mut P,
    config: &CpuConfig,
    payload: &[u8],
) -> Result<(), Error> {
    let p = ActivateModBankPayload::parse(payload)?;
    activate_bank(
        port,
        &MOD_BANK_REGS,
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
        ADDR_MOD_REQ_RD_BANK, ADDR_MOD_TRANSITION_MODE, ADDR_MOD_TRANSITION_VALUE_0, CtlFlags,
    };
    use crate::proto::Error;
    use crate::test_utils::builders::{activate_mod_bank, config_mod_rep, invalid_bank};
    use crate::test_utils::mock::Harness;

    const SYS_TIME_TRANSITION_MARGIN_NS: u64 =
        autd3_cpu_wire::cpu_params::SYS_TIME_TRANSITION_MARGIN.as_nanos() as u64;

    #[test]
    fn activate_mod_bank_writes_transition_and_req_bank_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::MOD_SET);

        h.deliver(&activate_mod_bank(0, 1, TransitionMode::Immediate, 0));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_MOD_TRANSITION_MODE),
            TransitionMode::Immediate as u16
        );
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
        assert_eq!(h.latch_count(CtlFlags::MOD_SET), latches_at_boot + 1);
        assert!(!h.ctl_flags().contains(CtlFlags::MOD_SET));
    }

    #[test]
    fn activate_mod_bank_rejects_invalid_bank() {
        let mut h = Harness::new();
        h.deliver(&activate_mod_bank(
            0,
            invalid_bank(),
            TransitionMode::Immediate,
            0,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
    }

    #[test]
    fn activate_mod_bank_rejects_timed_transition_on_infinite_loop() {
        let mut h = Harness::new();

        h.deliver(&activate_mod_bank(0, 1, TransitionMode::SyncIdx, 0));
        assert_eq!(h.status(), Error::InvalidTransitionMode);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

        h.deliver(&activate_mod_bank(1, 1, TransitionMode::Ext, 0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    }

    #[test]
    fn activate_mod_bank_rejects_immediate_transition_on_finite_loop() {
        let mut h = Harness::new();
        h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_mod_bank(1, 1, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::InvalidTransitionMode);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

        h.deliver(&activate_mod_bank(2, 1, TransitionMode::Gpio, 1));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    }

    #[test]
    fn activate_mod_bank_writes_gpio_pin_unconverted() {
        let mut h = Harness::new();
        h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_mod_bank(1, 1, TransitionMode::Gpio, 3));

        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_TRANSITION_MODE), TransitionMode::Gpio as u16);
        assert_eq!(h.ctl(ADDR_MOD_TRANSITION_VALUE_0), 3);
        assert_eq!(h.ctl(ADDR_MOD_TRANSITION_VALUE_0 + 1), 0);
        assert_eq!(h.ctl(ADDR_MOD_TRANSITION_VALUE_0 + 2), 0);
        assert_eq!(h.ctl(ADDR_MOD_TRANSITION_VALUE_0 + 3), 0);
    }

    #[test]
    fn activate_mod_bank_rejects_sys_time_transition_within_margin() {
        let mut h = Harness::new();
        h.deliver(&config_mod_rep(0, 1, 10, 100, 4));
        assert_eq!(h.status(), Error::None);
        h.port.sys_time = Some(1_000_000_000);

        h.deliver(&activate_mod_bank(
            1,
            1,
            TransitionMode::SysTime,
            1_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS - 1,
        ));
        assert_eq!(h.status(), Error::MissTransitionTime);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

        h.deliver(&activate_mod_bank(
            2,
            1,
            TransitionMode::SysTime,
            1_000_000_000 + SYS_TIME_TRANSITION_MARGIN_NS,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    }
}
