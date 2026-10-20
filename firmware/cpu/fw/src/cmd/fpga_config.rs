pub(crate) use autd3_cpu_wire::payload::SetFpgaConfigPayload;

use crate::fpga;
use crate::fpga_params::FunctionBits;
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let config = SetFpgaConfigPayload::parse(payload)?;
    if !fpga::functions(port).contains(FunctionBits::SYNC_PHASE_LOCK) {
        return Err(Error::FpgaUnsupported);
    }
    fpga::apply_fpga_config(port, &config);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::fpga_params::{ADDR_FUNCTION_BITS, FunctionBits};
    use crate::proto::Error;
    use crate::test_utils::builders::fpga_config;
    use crate::test_utils::mock::Harness;

    #[test]
    fn boot_applies_the_default_fpga_config() {
        let h = Harness::new();
        assert_eq!(h.port.sync_ps_ctl, Some(1));
    }

    #[test]
    fn fpga_config_writes_the_sync_phase_lock_bit() {
        let mut h = Harness::new();

        h.deliver(&fpga_config(0));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.sync_ps_ctl, Some(0));

        h.deliver(&fpga_config(1));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.sync_ps_ctl, Some(1));
    }

    #[test]
    fn fpga_config_rejects_out_of_range() {
        let mut h = Harness::new();
        h.port.sync_ps_ctl = None;
        h.deliver(&fpga_config(2));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.port.sync_ps_ctl, None);
    }

    #[test]
    fn fpga_config_is_unsupported_without_the_function_bit() {
        let mut h = Harness::new();
        h.port.sync_ps_ctl = None;
        h.set_ctl(
            ADDR_FUNCTION_BITS,
            u16::from(FunctionBits::CPU_WATCHDOG.bits()),
        );
        h.deliver(&fpga_config(0));
        assert_eq!(h.status(), Error::FpgaUnsupported);
        assert_eq!(h.port.sync_ps_ctl, None);
    }
}
