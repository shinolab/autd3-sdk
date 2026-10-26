pub(crate) use autd3_cpu_wire::config::CpuConfig;
pub(crate) use autd3_cpu_wire::payload::SetCpuConfigPayload;

use crate::app::Cpu;
use crate::fpga;
use crate::port::Port;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn config(&self) -> CpuConfig {
        self.config.get()
    }

    pub(crate) fn apply_config<P: Port>(&self, port: &mut P, config: CpuConfig) {
        self.config.set(config);
        port.configure_ptp(config.ptp);
        port.set_fpga_bus_wait(config.fpga_bus_wait);
        fpga::set_watchdog_timeout(port, config.cpu_watchdog_timeout);
    }

    pub(crate) fn set_cpu_config<P: Port>(
        &self,
        port: &mut P,
        payload: &[u8],
    ) -> Result<(), Error> {
        let config = SetCpuConfigPayload::parse(payload)?;
        self.apply_config(port, config);
        Ok(())
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use core::num::NonZeroU16;
    use core::time::Duration;

    use autd3_cpu_wire::config::{FpgaBusWait, PtpConfig};
    use zerocopy::IntoBytes;
    use zerocopy::little_endian::{U16, U32};

    use super::{CpuConfig, SetCpuConfigPayload};

    use crate::fpga_params::CtlFlags;
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::set_cpu_config;
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn the_boot_config_is_the_shared_default() {
        assert_eq!(Harness::new().cpu.config(), CpuConfig::DEFAULT);
    }

    #[test]
    fn invalid_values_are_rejected_without_changing_anything() {
        let valid = SetCpuConfigPayload {
            sync_guard_ns: U32::new(1),
            ..SetCpuConfigPayload::encode(&CpuConfig::DEFAULT).unwrap()
        };
        let rejected = [
            SetCpuConfigPayload {
                ptp_sync_interval_ms: U16::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                ptp_lock_samples: U16::new(0),
                ..valid
            },
        ];
        let mut h = Harness::new();
        for payload in &rejected {
            h.deliver(&Frame::from_payload(Cmd::SetCpuConfig, payload));
            assert_eq!(h.status(), Error::InvalidPayload);
            assert_eq!(h.cpu.config(), CpuConfig::DEFAULT);
            assert_eq!(h.port.ptp_config, None);
        }

        let bytes = valid.as_bytes();
        h.deliver(&Frame::from_payload(
            Cmd::SetCpuConfig,
            &bytes[..bytes.len() - 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.cpu.config(), CpuConfig::DEFAULT);
    }

    #[test]
    fn synchronize_passes_the_configured_guard_to_the_port() {
        let mut h = Harness::new();
        h.port.next_sync_edge = core::num::NonZeroU64::new(1_000_000);
        h.port.sys_time = Some(500_000);
        h.deliver(&Frame::new(Cmd::Synchronize));
        assert_eq!(h.port.sync_guard_ns, Some(250_000));

        h.deliver(&set_cpu_config(&CpuConfig {
            sync_guard: Duration::ZERO,
            ..CpuConfig::DEFAULT
        }));
        h.deliver(&Frame::new(Cmd::Synchronize));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.sync_guard_ns, Some(0));
        assert_eq!(h.latch_count(CtlFlags::SYNC_SET), 2);
    }

    #[test]
    fn the_ptp_values_reach_the_port() {
        let ptp = PtpConfig {
            sync_interval: Duration::from_millis(8),
            tx_timestamp_timeout: Duration::from_millis(2),
            delay_resp_timeout: Duration::from_millis(4),
            holdover: Duration::from_millis(500),
            lock_samples: NonZeroU16::new(32).unwrap(),
            step_threshold: Duration::from_micros(20),
            lock_threshold: Duration::from_nanos(200),
            kp_milli: 150,
            ki_milli: 30,
            max_freq_ppb: 100_000,
            delay_req_syncs: NonZeroU16::new(4).unwrap(),
            path_delay_filter_shift: 2,
            pause_quanta: None,
            pause_hold_syncs: 16,
            pause_retry: Duration::from_millis(4),
        };
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            ptp,
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.ptp_config, Some(ptp));
    }

    #[test]
    fn the_fpga_bus_wait_reaches_the_port_and_clear_restores_three_cycles() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            fpga_bus_wait: FpgaBusWait::Cycles2,
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.fpga_bus_wait, Some(FpgaBusWait::Cycles2));

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.fpga_bus_wait, Some(FpgaBusWait::Cycles3));
    }

    #[test]
    fn clear_restores_the_defaults() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            sync_guard: Duration::from_nanos(1),
            ptp: PtpConfig {
                holdover: Duration::from_millis(500),
                ..PtpConfig::default()
            },
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_ne!(h.cpu.config(), CpuConfig::DEFAULT);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.cpu.config(), CpuConfig::DEFAULT);
        assert_eq!(h.port.ptp_config, Some(PtpConfig::default()));
    }
}
