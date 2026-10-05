pub use autd3_cpu_wire::payload::ForceFanPayload;

use crate::fpga;
use crate::fpga_params::{ADDR_CTL_FLAG, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let p = ForceFanPayload::parse(payload)?;
    let mut ctl = CtlFlags::from_bits_retain(fpga::read_ctl(port, ADDR_CTL_FLAG));
    ctl.set(CtlFlags::FORCE_FAN, p.value);
    fpga::write_ctl(port, ADDR_CTL_FLAG, ctl.bits());
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use crate::cmd::silencer::SilencerFlags;
    use crate::fpga_params::CtlFlags;
    use crate::proto::Error;
    use crate::test_utils::builders::{force_fan, set_silencer};
    use crate::test_utils::mock::Harness;

    #[test]
    fn force_fan_sets_and_clears_persistent_bit() {
        let mut h = Harness::new();

        h.deliver(&force_fan(0, 1));
        assert_eq!(h.status(), Error::None);
        assert!(h.ctl_flags().contains(CtlFlags::FORCE_FAN));

        h.deliver(&force_fan(1, 0));
        assert_eq!(h.status(), Error::None);
        assert!(!h.ctl_flags().contains(CtlFlags::FORCE_FAN));
    }

    #[test]
    fn force_fan_rejects_out_of_range() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 2));
        assert_eq!(h.status(), Error::InvalidPayload);
    }

    #[test]
    fn force_fan_survives_subsequent_latch() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 1));
        h.deliver(&set_silencer(1, SilencerFlags::STRICT_MODE, 256, 256, 5, 7));
        assert!(h.ctl_flags().contains(CtlFlags::FORCE_FAN));
    }
}
