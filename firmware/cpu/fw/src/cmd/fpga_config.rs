pub(crate) use autd3_cpu_wire::payload::SetFpgaConfigPayload;

use crate::fpga;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let config = SetFpgaConfigPayload::parse(payload)?;
    fpga::apply_fpga_config(port, &config);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::proto::Error;
    use crate::test_utils::builders::fpga_config;
    use crate::test_utils::mock::Harness;

    #[test]
    fn boot_applies_the_default_fpga_config() {
        let h = Harness::new();
        assert_eq!(h.port.sync_ps_ctl, Some(1));
        assert_eq!(h.port.sync_ps_gains, [Some(64), Some(1024), Some(4096)]);
    }

    #[test]
    fn fpga_config_writes_the_sync_phase_lock_bit_and_gains() {
        let mut h = Harness::new();

        h.deliver(&fpga_config(0, [11, 22, u16::MAX]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.sync_ps_ctl, Some(0));
        assert_eq!(h.port.sync_ps_gains, [Some(11), Some(22), Some(u16::MAX)]);

        h.deliver(&fpga_config(1, [0, 0, 0]));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.sync_ps_ctl, Some(1));
        assert_eq!(h.port.sync_ps_gains, [Some(0), Some(0), Some(0)]);
    }

    #[test]
    fn fpga_config_rejects_out_of_range() {
        let mut h = Harness::new();
        h.port.sync_ps_ctl = None;
        h.port.sync_ps_gains = [None; 3];
        h.deliver(&fpga_config(2, [1, 2, 3]));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.port.sync_ps_ctl, None);
        assert_eq!(h.port.sync_ps_gains, [None; 3]);
    }
}
