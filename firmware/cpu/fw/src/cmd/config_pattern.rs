pub use autd3_cpu_wire::payload::ConfigPatternPayload;

use crate::fpga;
use crate::fpga_params::{
    ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
    ADDR_PATTERN_REP0, ADDR_PATTERN_SOUND_SPEED0,
};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let cfg = ConfigPatternPayload::parse(payload)?;
    let bank_offset = u16::from(cfg.bank.as_u8());
    fpga::write_ctl(
        port,
        ADDR_PATTERN_MODE0 + bank_offset,
        cfg.emission_type as u16,
    );
    fpga::write_ctl(
        port,
        ADDR_PATTERN_CYCLE0 + bank_offset,
        (cfg.size.get() - 1) as u16,
    );
    fpga::write_ctl(
        port,
        ADDR_PATTERN_FREQ_DIV0 + bank_offset,
        cfg.divider.get(),
    );
    fpga::write_ctl(
        port,
        ADDR_PATTERN_SOUND_SPEED0 + bank_offset,
        cfg.sound_speed.get(),
    );
    fpga::write_ctl(
        port,
        ADDR_PATTERN_NUM_FOCI0 + bank_offset,
        cfg.num_foci.map_or(0, |n| u16::from(n.get())),
    );
    fpga::write_ctl(port, ADDR_PATTERN_REP0 + bank_offset, cfg.rep.get());
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga::{REP_INFINITE, TransitionMode};
    use crate::fpga_params::{
        ADDR_PATTERN_CYCLE0, ADDR_PATTERN_FREQ_DIV0, ADDR_PATTERN_MODE0, ADDR_PATTERN_NUM_FOCI0,
        ADDR_PATTERN_REP0, ADDR_PATTERN_REQ_RD_BANK, ADDR_PATTERN_SOUND_SPEED0,
        ADDR_PATTERN_TRANSITION_MODE, CtlFlags, EMISSION_MAX_INDICES, EmissionType, NUM_FOCI_MAX,
    };
    use crate::proto::{BUFFER_SIZE_MIN, Error, MAX_FOCI_TOTAL};
    use crate::test_utils::builders::{config_pattern, config_pattern_rep};
    use crate::test_utils::mock::Harness;

    #[test]
    fn config_pattern_allows_single_index_only_for_infinite_loop() {
        let mut h = Harness::new();
        h.deliver(&config_pattern(
            0,
            0,
            EmissionType::Raw.as_u8(),
            10,
            1,
            0,
            0,
        ));
        assert_eq!(h.status(), Error::None, "static pattern is a single index");

        h.deliver(&config_pattern_rep(
            1,
            0,
            EmissionType::Raw.as_u8(),
            10,
            1,
            0,
            0,
            4,
        ));
        assert_eq!(
            h.status(),
            Error::InvalidPayload,
            "a single index never advances, so a finite loop would never end"
        );

        h.deliver(&config_pattern_rep(
            2,
            0,
            EmissionType::Raw.as_u8(),
            10,
            BUFFER_SIZE_MIN,
            0,
            0,
            4,
        ));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn config_pattern_raw_writes_registers_without_latching() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::PATTERN_SET);

        h.deliver(&config_pattern(
            0,
            0,
            EmissionType::Raw.as_u8(),
            2,
            EMISSION_MAX_INDICES,
            0,
            0,
        ));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_MODE0),
            u16::from(EmissionType::Raw.as_u8())
        );
        assert_eq!(
            h.ctl(ADDR_PATTERN_CYCLE0),
            u16::try_from(EMISSION_MAX_INDICES - 1).unwrap()
        );
        assert_eq!(h.ctl(ADDR_PATTERN_FREQ_DIV0), 2);
        assert_eq!(h.ctl(ADDR_PATTERN_REP0), REP_INFINITE);
        assert_eq!(
            h.ctl(ADDR_PATTERN_TRANSITION_MODE),
            TransitionMode::SyncIdx as u16
        );
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
        assert_eq!(h.latch_count(CtlFlags::PATTERN_SET), latches_at_boot);
        assert!(!h.ctl_flags().contains(CtlFlags::PATTERN_SET));
    }

    #[test]
    fn config_pattern_foci_writes_registers() {
        let mut h = Harness::new();

        h.deliver(&config_pattern(
            0,
            1,
            EmissionType::Foci.as_u8(),
            1,
            8192,
            8,
            340,
        ));

        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.ctl(ADDR_PATTERN_MODE0 + 1),
            u16::from(EmissionType::Foci.as_u8())
        );
        assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0 + 1), 8191);
        assert_eq!(h.ctl(ADDR_PATTERN_SOUND_SPEED0 + 1), 340);
        assert_eq!(h.ctl(ADDR_PATTERN_NUM_FOCI0 + 1), 8);
        assert_eq!(h.ctl(ADDR_PATTERN_REP0 + 1), REP_INFINITE);
        assert_eq!(h.ctl(ADDR_PATTERN_REQ_RD_BANK), 0);
        assert!(!h.ctl_flags().contains(CtlFlags::PATTERN_SET));
    }

    #[test]
    fn config_pattern_writes_finite_loop_rep() {
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
        assert_eq!(h.ctl(ADDR_PATTERN_REP0), 4);
    }

    #[test]
    fn config_pattern_rejects_invalid_raw_fields() {
        let mut h = Harness::new();

        h.deliver(&config_pattern(
            0,
            0,
            EmissionType::Raw.as_u8(),
            1,
            EMISSION_MAX_INDICES + 1,
            0,
            0,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&config_pattern(1, 0, 2, 1, 1, 0, 0));
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_eq!(h.ctl(ADDR_PATTERN_CYCLE0), 0);
    }

    #[test]
    fn config_pattern_rejects_invalid_foci_fields() {
        let mut h = Harness::new();

        h.deliver(&config_pattern(
            0,
            0,
            EmissionType::Foci.as_u8(),
            1,
            2,
            0,
            340,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&config_pattern(
            1,
            0,
            EmissionType::Foci.as_u8(),
            1,
            2,
            NUM_FOCI_MAX + 1,
            340,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&config_pattern(
            5,
            0,
            EmissionType::Foci.as_u8(),
            1,
            1,
            1,
            340,
        ));
        assert_eq!(
            h.status(),
            Error::InvalidPayload,
            "a single-sample STM never advances its index"
        );

        h.deliver(&config_pattern(
            2,
            0,
            EmissionType::Foci.as_u8(),
            1,
            MAX_FOCI_TOTAL / 8 + 1,
            8,
            340,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&config_pattern(
            3,
            0,
            EmissionType::Foci.as_u8(),
            1,
            2,
            1,
            0,
        ));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&config_pattern(
            4,
            0,
            EmissionType::Foci.as_u8(),
            1,
            MAX_FOCI_TOTAL / 8,
            8,
            340,
        ));
        assert_eq!(h.status(), Error::None);
    }
}
