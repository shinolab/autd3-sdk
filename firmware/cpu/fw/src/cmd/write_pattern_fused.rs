pub(crate) use autd3_cpu_wire::payload::WritePatternFusedPayload;

use autd3_cpu_wire::payload::ConfigPatternPayload;
use zerocopy::little_endian::{U16, U32};

use crate::cmd::bank::{PATTERN_BANK_REGS, activate_bank};
use crate::cmd::{config_pattern, write_pattern_raw};
use crate::fpga::{EmissionType, REP_INFINITE};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let (p, data) = WritePatternFusedPayload::parse(payload)?;
    write_pattern_raw::write_slot(port, p.bank, 0, data);
    config_pattern::write(
        port,
        &ConfigPatternPayload {
            bank: p.bank,
            emission_type: EmissionType::Raw,
            divider: U16::new(u16::MAX),
            size: U32::new(1),
            num_foci: None,
            reserved: 0,
            sound_speed: U16::new(0),
            rep: U16::new(REP_INFINITE),
        },
    );
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
    use std::vec::Vec;

    use rstest::rstest;

    use super::WritePatternFusedPayload;
    use crate::fpga::TransitionModeKind;
    use crate::fpga_params::{
        ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_TRANSITION_MODE, CtlFlags, EmissionType,
        NUM_TRANSDUCERS,
    };
    use crate::proto::Error;
    use crate::test_utils::builders::{
        activate_pattern_bank, assert_fpga_unchanged, config_pattern, fpga_snapshot, invalid_bank,
        write_pattern_fused, write_pattern_raw,
    };
    use crate::test_utils::mock::Harness;

    fn raw_pattern() -> (Vec<u8>, Vec<u8>) {
        let phases = (0..NUM_TRANSDUCERS).map(|i| i as u8).collect();
        let intensities = (0..NUM_TRANSDUCERS).map(|i| 0xFF - i as u8).collect();
        (phases, intensities)
    }

    #[rstest]
    #[case(0, TransitionModeKind::Immediate)]
    #[case(1, TransitionModeKind::Immediate)]
    #[case(1, TransitionModeKind::Ext)]
    fn the_fused_frame_leaves_the_fpga_as_write_config_activate_does(
        #[case] bank: u8,
        #[case] mode: TransitionModeKind,
    ) {
        let (phases, intensities) = raw_pattern();

        let mut fused = Harness::new();
        fused.deliver(&write_pattern_fused(bank, mode, 0, &phases, &intensities));
        assert_eq!(fused.status(), Error::None);

        let mut split = Harness::new();
        split.deliver(&write_pattern_raw(bank, 0, &phases, &intensities));
        assert_eq!(split.status(), Error::None);
        split.deliver(&config_pattern(
            bank,
            EmissionType::Raw.as_u8(),
            u16::MAX,
            1,
            0,
            0,
        ));
        assert_eq!(split.status(), Error::None);
        split.deliver(&activate_pattern_bank(bank, mode, 0));
        assert_eq!(split.status(), Error::None);

        assert_eq!(fpga_snapshot(&fused), fpga_snapshot(&split));
        assert_eq!(fused.ctl(ADDR_PATTERN_REQ_RD_BANK), u16::from(bank));
    }

    #[test]
    fn a_malformed_frame_touches_nothing() {
        let (phases, intensities) = raw_pattern();
        let mut h = Harness::new();
        let before = fpga_snapshot(&h);

        h.deliver(&write_pattern_fused(
            invalid_bank(),
            TransitionModeKind::Immediate,
            0,
            &phases,
            &intensities,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut short =
            write_pattern_fused(0, TransitionModeKind::Immediate, 0, &phases, &intensities);
        short.set_len(size_of::<WritePatternFusedPayload>() + 2 * NUM_TRANSDUCERS - 1);
        h.deliver(&short);
        assert_eq!(h.status(), Error::InvalidPayload);

        let mut unknown_mode =
            write_pattern_fused(0, TransitionModeKind::Immediate, 0, &phases, &intensities);
        unknown_mode.set_payload_byte(1, 0x03);
        h.deliver(&unknown_mode);
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_fpga_unchanged(&before, &h);
    }

    #[rstest]
    #[case(TransitionModeKind::SyncIdx, 0)]
    #[case(TransitionModeKind::SysTime, 4_000_000_000)]
    #[case(TransitionModeKind::Gpio, 1)]
    fn a_transition_for_a_finite_loop_is_written_as_is(
        #[case] mode: TransitionModeKind,
        #[case] value: u64,
    ) {
        let (phases, intensities) = raw_pattern();
        let mut h = Harness::new();

        h.deliver(&write_pattern_fused(1, mode, value, &phases, &intensities));

        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_PATTERN_TRANSITION_MODE), mode as u16);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
    }

    #[test]
    fn a_latch_the_fpga_rejects_reports_invalid_silencer_setting() {
        let (phases, intensities) = raw_pattern();
        let mut h = Harness::new();
        h.port.reject_latch = CtlFlags::PATTERN_SET;

        h.deliver(&write_pattern_fused(
            1,
            TransitionModeKind::Immediate,
            0,
            &phases,
            &intensities,
        ));

        assert_eq!(h.status(), Error::InvalidSilencerSetting);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 1);
        assert_eq!(h.latched(ADDR_PATTERN_REQ_RD_BANK), 0);
    }
}
