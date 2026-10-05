pub use autd3_cpu_wire::payload::{SilencerFlags, SilencerPayload};

use core::num::NonZeroU32;

use crate::fpga;
use crate::fpga_params::{
    ADDR_SILENCER_COMPLETION_STEPS_INTENSITY, ADDR_SILENCER_COMPLETION_STEPS_PHASE,
    ADDR_SILENCER_FLAG, ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE,
    CtlFlags, FunctionBits,
};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(
    port: &mut P,
    payload: &[u8],
    max_polls: NonZeroU32,
) -> Result<(), Error> {
    let p = SilencerPayload::parse(payload)?;
    let flag = p.flags();
    let update_rate_intensity = p.update_rate_intensity.get();
    let update_rate_phase = p.update_rate_phase.get();
    let completion_steps_intensity = p.completion_steps_intensity.get();
    let completion_steps_phase = p.completion_steps_phase.get();

    let strict = !flag.contains(SilencerFlags::FIXED_UPDATE_RATE_MODE)
        && flag.contains(SilencerFlags::STRICT_MODE);
    if strict && !fpga::functions(port).contains(FunctionBits::STRICT_SILENCER_GUARD) {
        return Err(Error::InvalidSilencerSetting);
    }

    fpga::write_ctl(
        port,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY,
        update_rate_intensity,
    );
    fpga::write_ctl(port, ADDR_SILENCER_UPDATE_RATE_PHASE, update_rate_phase);
    fpga::write_ctl(
        port,
        ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        completion_steps_intensity,
    );
    fpga::write_ctl(
        port,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE,
        completion_steps_phase,
    );
    fpga::write_ctl(port, ADDR_SILENCER_FLAG, u16::from(flag.bits()));
    fpga::set_and_wait_update(port, CtlFlags::SILENCER_SET, max_polls)
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::SilencerFlags;
    use crate::fpga::TransitionMode;
    use crate::fpga_params::{
        ADDR_FUNCTION_BITS, ADDR_SILENCER_COMPLETION_STEPS_INTENSITY,
        ADDR_SILENCER_COMPLETION_STEPS_PHASE, ADDR_SILENCER_FLAG,
        ADDR_SILENCER_UPDATE_RATE_INTENSITY, ADDR_SILENCER_UPDATE_RATE_PHASE, CtlFlags,
    };
    use crate::proto::Error;
    use crate::test_utils::builders::{activate_mod_bank, activate_pattern_bank, set_silencer};
    use crate::test_utils::mock::Harness;

    #[test]
    fn set_silencer_fixed_completion_steps_writes_registers_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::SILENCER_SET);

        h.deliver(&set_silencer(0, SilencerFlags::STRICT_MODE, 256, 256, 5, 7));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_SILENCER_FLAG),
            u16::from(SilencerFlags::STRICT_MODE.bits())
        );
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 7);
        assert_eq!(h.latch_count(CtlFlags::SILENCER_SET), latches_at_boot + 1);
        assert!(!h.ctl_flags().contains(CtlFlags::SILENCER_SET));
    }

    #[test]
    fn set_silencer_fixed_update_rate_writes_registers_and_latches() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::SILENCER_SET);

        h.deliver(&set_silencer(
            0,
            SilencerFlags::FIXED_UPDATE_RATE_MODE,
            8,
            16,
            10,
            40,
        ));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_SILENCER_FLAG),
            u16::from(SilencerFlags::FIXED_UPDATE_RATE_MODE.bits())
        );
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 8);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 16);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
        assert_eq!(h.latch_count(CtlFlags::SILENCER_SET), latches_at_boot + 1);
    }

    #[test]
    fn set_silencer_rejects_zero_completion_steps_in_steps_mode() {
        let mut h = Harness::new();

        h.deliver(&set_silencer(0, SilencerFlags::empty(), 256, 256, 0, 7));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&set_silencer(1, SilencerFlags::empty(), 256, 256, 5, 0));
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_PHASE), 40);
    }

    #[test]
    fn set_silencer_rejects_zero_update_rate_in_rate_mode() {
        let mut h = Harness::new();

        h.deliver(&set_silencer(
            0,
            SilencerFlags::FIXED_UPDATE_RATE_MODE,
            0,
            16,
            10,
            40,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&set_silencer(
            1,
            SilencerFlags::FIXED_UPDATE_RATE_MODE,
            8,
            0,
            10,
            40,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_PHASE), 256);
        assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
    }

    #[test]
    fn set_silencer_steps_mode_ignores_zero_update_rate() {
        let mut h = Harness::new();

        h.deliver(&set_silencer(0, SilencerFlags::empty(), 0, 0, 5, 7));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_SILENCER_UPDATE_RATE_INTENSITY), 0);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 5);
    }

    #[test]
    fn strict_silencer_is_refused_on_an_fpga_without_the_guard() {
        let mut h = Harness::new();
        h.set_ctl(ADDR_FUNCTION_BITS, 0);

        h.deliver(&set_silencer(
            0,
            SilencerFlags::STRICT_MODE,
            256,
            256,
            8,
            40,
        ));
        assert_eq!(h.status(), Error::InvalidSilencerSetting);
        assert_eq!(h.ctl(ADDR_SILENCER_FLAG), 0);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);

        h.deliver(&set_silencer(1, SilencerFlags::empty(), 256, 256, 8, 40));
        assert_eq!(h.status(), Error::None);
        h.deliver(&set_silencer(
            2,
            SilencerFlags::STRICT_MODE | SilencerFlags::FIXED_UPDATE_RATE_MODE,
            8,
            16,
            10,
            40,
        ));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn a_latch_the_fpga_rejects_reports_invalid_silencer_setting() {
        let mut h = Harness::new();
        h.port.reject_latch = CtlFlags::SILENCER_SET | CtlFlags::MOD_SET | CtlFlags::PATTERN_SET;

        h.deliver(&set_silencer(0, SilencerFlags::empty(), 256, 256, 8, 40));
        assert_eq!(h.status(), Error::InvalidSilencerSetting);
        assert_eq!(h.ctl(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 8);
        assert_eq!(h.latched(ADDR_SILENCER_COMPLETION_STEPS_INTENSITY), 10);

        h.deliver(&activate_mod_bank(1, 0, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::InvalidSilencerSetting);
        h.deliver(&activate_pattern_bank(2, 0, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::InvalidSilencerSetting);

        h.port.reject_latch = CtlFlags::empty();
        h.deliver(&activate_mod_bank(3, 0, TransitionMode::Immediate, 0));
        assert_eq!(h.status(), Error::None);
    }
}
