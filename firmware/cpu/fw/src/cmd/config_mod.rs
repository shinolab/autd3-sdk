pub use autd3_cpu_wire::payload::ConfigModPayload;

use crate::fpga;
use crate::fpga_params::{ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let cfg = ConfigModPayload::parse(payload)?;
    let bank_offset = u16::from(cfg.bank.as_u8());
    fpga::write_ctl(
        port,
        ADDR_MOD_CYCLE0 + bank_offset,
        (cfg.size.get() - 1) as u16,
    );
    fpga::write_ctl(port, ADDR_MOD_FREQ_DIV0 + bank_offset, cfg.divider.get());
    fpga::write_ctl(port, ADDR_MOD_REP0 + bank_offset, cfg.rep.get());
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga::{REP_INFINITE, TransitionMode};
    use crate::fpga_params::{
        ADDR_MOD_CYCLE0, ADDR_MOD_FREQ_DIV0, ADDR_MOD_REP0, ADDR_MOD_REQ_RD_BANK,
        ADDR_MOD_TRANSITION_MODE, CtlFlags,
    };
    use crate::proto::{Error, MOD_BUFFER_SAMPLES};
    use crate::test_utils::builders::{config_mod, config_mod_rep, invalid_bank};
    use crate::test_utils::mock::Harness;

    const BUFFER_SIZE_MIN: u32 = autd3_cpu_wire::layout::BUFFER_SIZE_MIN as u32;

    #[test]
    fn config_mod_writes_playback_registers_without_latching() {
        let mut h = Harness::new();
        let latches_at_boot = h.latch_count(CtlFlags::MOD_SET);

        h.deliver(&config_mod(0, 1, 10, 4000));

        assert_eq!(h.ack(), 0);
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 3999);
        assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + 1), 10);
        assert_eq!(h.ctl(ADDR_MOD_REP0 + 1), REP_INFINITE);
        assert_eq!(
            h.ctl(ADDR_MOD_TRANSITION_MODE),
            TransitionMode::SyncIdx as u16
        );
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
        assert_eq!(h.latch_count(CtlFlags::MOD_SET), latches_at_boot);
        assert!(!h.ctl_flags().contains(CtlFlags::MOD_SET));
    }

    #[test]
    fn config_mod_writes_finite_loop_rep() {
        let mut h = Harness::new();

        h.deliver(&config_mod_rep(0, 0, 10, 4000, 9));

        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REP0), 9);
    }

    #[test]
    fn config_mod_rejects_invalid_fields_and_leaves_registers_untouched() {
        let mut h = Harness::new();
        h.deliver(&config_mod(0, 1, 2, 100));
        assert_eq!(h.status(), Error::None);

        h.deliver(&config_mod(1, invalid_bank(), 1, 1));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&config_mod(2, 0, 0, 1));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&config_mod(3, 0, 1, 0));
        assert_eq!(h.status(), Error::InvalidPayload);
        h.deliver(&config_mod(4, 0, 1, MOD_BUFFER_SAMPLES + 1));
        assert_eq!(h.status(), Error::InvalidPayload);

        assert_eq!(h.ctl(ADDR_MOD_CYCLE0 + 1), 99);
        assert_eq!(h.ctl(ADDR_MOD_FREQ_DIV0 + 1), 2);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);
    }

    #[test]
    fn config_mod_rejects_single_sample_buffer() {
        let mut h = Harness::new();
        h.deliver(&config_mod(0, 0, 1, 1));
        assert_eq!(h.status(), Error::InvalidPayload);

        h.deliver(&config_mod(1, 0, 1, BUFFER_SIZE_MIN));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn config_mod_accepts_full_buffer_size() {
        let mut h = Harness::new();
        h.deliver(&config_mod(0, 0, 1, MOD_BUFFER_SAMPLES));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_CYCLE0), 0xFFFF);
    }
}
