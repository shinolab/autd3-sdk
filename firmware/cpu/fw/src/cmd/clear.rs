use crate::app::Cpu;
use crate::cmd::cpu_config::default_config;
use crate::fpga;
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn clear<P: Port>(&self, port: &mut P) -> Result<(), Error> {
        self.apply_config(port, default_config());
        let result = fpga::init(port, self.config().fpga_wait_update_max_polls);
        self.reset_telemetry();
        result
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::cmd::silencer::SilencerFlags;
    use crate::fpga::TransitionMode;
    use crate::fpga_params::{
        ADDR_MOD_FREQ_DIV0, ADDR_MOD_REQ_RD_BANK, ADDR_PATTERN_FREQ_DIV0,
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        ADDR_SILENCER_FLAG, CtlFlags, NUM_BANKS,
    };
    use crate::proto::{Cmd, Error, Telemetry};
    use crate::test_utils::builders::{activate_mod_bank, config_mod, force_fan, set_silencer};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn clear_resets_telemetry_counters() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 2));
        assert_eq!(h.telemetry(Telemetry::DispatchError), 1);

        h.deliver(&Frame::new(1, Cmd::Clear));
        assert_eq!(h.telemetry(Telemetry::DispatchError), 0);
    }

    #[test]
    fn clear_reports_fpga_timeout_when_latch_stuck() {
        let mut h = Harness::new();
        h.port.latch_stuck = true;

        h.deliver(&Frame::new(0, Cmd::Clear));
        assert_eq!(h.status(), Error::FpgaTimeout);

        h.port.latch_stuck = false;
    }

    #[test]
    fn clear_latches_the_silencer_before_the_bank_settings() {
        let mut h = Harness::new();
        h.port.latch_log.clear();
        h.deliver(&Frame::new(0, Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.port.latch_log,
            [
                CtlFlags::SILENCER_SET,
                CtlFlags::MOD_SET | CtlFlags::PATTERN_SET | CtlFlags::DEBUG_SET,
            ]
        );
    }

    #[test]
    fn clear_restores_silencer_and_bank_baseline() {
        let mut h = Harness::new();
        h.deliver(&set_silencer(
            0,
            SilencerFlags::STRICT_MODE,
            256,
            256,
            20,
            30,
        ));
        assert_eq!(h.status(), Error::None);
        h.deliver(&config_mod(1, 1, 50, 100));
        assert_eq!(h.status(), Error::None);
        h.deliver(&activate_mod_bank(2, 1, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::None);

        h.deliver(&Frame::new(3, Cmd::Clear));
        assert_eq!(h.status(), Error::None);

        assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
        for bank in 0..u16::try_from(NUM_BANKS).unwrap() {
            assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + bank), 0xFFFF);
            assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0 + bank), 0xFFFF);
        }
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
    }

    #[test]
    fn clear_resets_force_fan() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 1));
        h.deliver(&Frame::new(1, Cmd::Clear));
        assert!(!h.ctl_flags().contains(CtlFlags::FORCE_FAN));
    }
}
