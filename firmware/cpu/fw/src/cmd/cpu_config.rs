pub(crate) use autd3_cpu_wire::config::{CpuConfig, FpgaBusWait};
pub(crate) use autd3_cpu_wire::payload::SetCpuConfigPayload;

use autd3_cpu_wire::cpu_params;

use core::sync::atomic::Ordering;

use crate::app::Cpu;
use crate::fpga;
use crate::port::Port;
use crate::proto::Error;
use crate::ptp;

#[cfg(not(test))]
const FPGA_FLASH_MAX_POLLS: core::num::NonZeroU32 = cpu_params::FPGA_FLASH_MAX_POLLS;
#[cfg(test)]
const FPGA_FLASH_MAX_POLLS: core::num::NonZeroU32 = core::num::NonZeroU32::new(10_000).unwrap();

pub(crate) const fn default_config() -> CpuConfig {
    CpuConfig {
        sys_time_transition_margin: cpu_params::SYS_TIME_TRANSITION_MARGIN,
        fpga_wait_update_max_polls: cpu_params::FPGA_WAIT_UPDATE_MAX_POLLS,
        fpga_flash_max_polls: FPGA_FLASH_MAX_POLLS,
        sync_guard: cpu_params::SYNC_GUARD,
        update_activate_delay: cpu_params::UPDATE_ACTIVATE_DELAY,
        failsafe_timeout: None,
        ptp_unlock_failsafe_timeout: None,
        fpga_bus_wait: FpgaBusWait::Cycles3,
        cpu_watchdog_timeout: Some(cpu_params::CPU_WATCHDOG_TIMEOUT),
        ptp: ptp::default_config(),
    }
}

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

    pub fn request_config_reset(&self) {
        self.config_reset_requested.store(true, Ordering::Release);
    }

    pub(crate) fn apply_requested_config_reset<P: Port>(&self, port: &mut P) {
        if self.config_reset_requested.swap(false, Ordering::AcqRel) {
            self.apply_config(port, default_config());
        }
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
    use core::num::{NonZeroU16, NonZeroU32};
    use core::time::Duration;

    use autd3_cpu_wire::config::{MaxFreqPpb, PathDelayFilterShift, PtpConfig};
    use zerocopy::IntoBytes;
    use zerocopy::little_endian::{U16, U32};

    use super::{CpuConfig, FpgaBusWait, SetCpuConfigPayload, default_config};

    const DEFAULT: CpuConfig = default_config();
    use crate::fpga::TransitionMode;
    use crate::fpga_params::{ADDR_CTL_FLAG, ADDR_MOD_REQ_RD_BANK, CtlFlags};
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::{activate_mod_bank, config_mod_rep, set_cpu_config};
    use crate::test_utils::mock::{Frame, Harness};

    #[test]
    fn the_boot_config_is_the_shared_default() {
        assert_eq!(
            default_config(),
            CpuConfig {
                fpga_flash_max_polls: super::FPGA_FLASH_MAX_POLLS,
                ..CpuConfig::default()
            }
        );
        assert_eq!(Harness::new().cpu.config(), default_config());
    }

    #[test]
    fn the_margin_follows_the_config() {
        let mut h = Harness::new();
        h.deliver(&config_mod_rep(1, 10, 100, 4));
        h.port.sys_time = Some(1_000_000_000);

        h.deliver(&set_cpu_config(&CpuConfig {
            sys_time_transition_margin: Duration::from_millis(1),
            ..DEFAULT
        }));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_mod_bank(
            1,
            TransitionMode::SysTime,
            1_000_000_000 + 999_999,
        ));
        assert_eq!(h.status(), Error::MissTransitionTime);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 0);

        h.deliver(&activate_mod_bank(
            1,
            TransitionMode::SysTime,
            1_000_000_000 + 1_000_000,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    }

    #[test]
    fn a_zero_margin_means_no_margin() {
        let mut h = Harness::new();
        h.deliver(&config_mod_rep(1, 10, 100, 4));
        h.port.sys_time = Some(1_000_000_000);
        h.deliver(&set_cpu_config(&CpuConfig {
            sys_time_transition_margin: Duration::ZERO,
            ..DEFAULT
        }));
        assert_eq!(h.status(), Error::None);

        h.deliver(&activate_mod_bank(1, TransitionMode::SysTime, 999_999_999));
        assert_eq!(h.status(), Error::MissTransitionTime);

        h.deliver(&activate_mod_bank(
            1,
            TransitionMode::SysTime,
            1_000_000_000,
        ));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl(ADDR_MOD_REQ_RD_BANK), 1);
    }

    #[test]
    fn invalid_values_are_rejected_without_changing_anything() {
        let valid = SetCpuConfigPayload {
            sys_time_transition_margin_ns: U32::new(1),
            ..SetCpuConfigPayload::encode(&DEFAULT).unwrap()
        };
        let rejected = [
            SetCpuConfigPayload {
                fpga_wait_update_max_polls: U32::new(0),
                ..valid
            },
            SetCpuConfigPayload {
                update_activate_delay_ms: U16::new(0),
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
            assert_eq!(h.cpu.config(), DEFAULT);
            assert_eq!(h.port.ptp_config, None);
        }

        let bytes = valid.as_bytes();
        h.deliver(&Frame::from_payload(
            Cmd::SetCpuConfig,
            &bytes[..bytes.len() - 1],
        ));
        assert_eq!(h.status(), Error::InvalidPayload);
        assert_eq!(h.cpu.config(), DEFAULT);
    }

    #[test]
    fn the_latch_wait_polls_the_configured_number_of_times() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            fpga_wait_update_max_polls: NonZeroU32::new(5).unwrap(),
            ..DEFAULT
        }));
        assert_eq!(h.status(), Error::None);

        h.port.latch_stuck = true;
        h.port.next_sync_edge = core::num::NonZeroU64::new(1_000_000);
        h.port.sys_time = Some(500_000);
        let reads = h.port.ctl_flag_reads;
        h.deliver(&Frame::new(Cmd::Synchronize));
        assert_eq!(h.status(), Error::FpgaTimeout);
        assert_eq!(h.port.ctl_flag_reads - reads, 1 + 5);
        h.port.latch_stuck = false;
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
            ..DEFAULT
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
            max_freq_ppb: MaxFreqPpb::new(100_000).unwrap(),
            delay_req_syncs: NonZeroU16::new(4).unwrap(),
            path_delay_filter_shift: PathDelayFilterShift::new(2).unwrap(),
            pause_quanta: None,
            pause_hold_syncs: 16,
            pause_retry: Duration::from_millis(4),
        };
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig { ptp, ..DEFAULT }));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.ptp_config, Some(ptp));
    }

    #[test]
    fn the_fpga_bus_wait_reaches_the_port_and_clear_restores_three_cycles() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            fpga_bus_wait: FpgaBusWait::Cycles2,
            ..DEFAULT
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
            sys_time_transition_margin: Duration::from_nanos(1),
            fpga_wait_update_max_polls: NonZeroU32::new(5).unwrap(),
            ptp: PtpConfig {
                holdover: Duration::from_millis(500),
                ..PtpConfig::default()
            },
            ..DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_ne!(h.cpu.config(), DEFAULT);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.cpu.config(), DEFAULT);
        assert_eq!(h.port.ptp_config, Some(PtpConfig::default()));
    }

    #[test]
    fn a_requested_reset_restores_the_defaults_on_the_next_tick() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            fpga_bus_wait: FpgaBusWait::Cycles2,
            ptp: PtpConfig {
                holdover: Duration::from_millis(500),
                ..PtpConfig::default()
            },
            ..DEFAULT
        }));
        assert_ne!(h.cpu.config(), DEFAULT);

        h.cpu.request_config_reset();
        assert_ne!(h.cpu.config(), DEFAULT);
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.cpu.config(), DEFAULT);
        assert_eq!(h.port.ptp_config, Some(PtpConfig::default()));
        assert_eq!(h.port.fpga_bus_wait, Some(FpgaBusWait::Cycles3));
    }

    #[test]
    fn a_requested_reset_is_applied_once() {
        let mut h = Harness::new();
        h.cpu.request_config_reset();
        h.cpu.tick_1ms(&mut h.port);

        let config = CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..DEFAULT
        };
        h.deliver(&set_cpu_config(&config));
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.cpu.config(), config);
    }

    #[test]
    fn a_requested_reset_lands_before_a_frame_queued_after_it() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            fpga_bus_wait: FpgaBusWait::Cycles2,
            ..DEFAULT
        }));

        h.cpu.request_config_reset();
        let config = CpuConfig {
            sys_time_transition_margin: Duration::from_nanos(1),
            ..DEFAULT
        };
        h.deliver(&set_cpu_config(&config));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.cpu.config(), config);
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.cpu.config(), config);
    }

    #[test]
    fn a_frame_queued_before_a_requested_reset_is_processed_after_it() {
        let mut h = Harness::new();
        let config = CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..DEFAULT
        };
        h.deliver_no_drain(&set_cpu_config(&config));
        h.cpu.request_config_reset();
        assert!(h.process_one());
        assert_eq!(h.cpu.config(), config);
    }

    #[test]
    fn a_requested_reset_disables_an_enabled_failsafe_and_keeps_the_gate_closed() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..DEFAULT
        }));
        h.port.host_idle_ms = Some(1000);
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.telemetry().failsafe, 1);
        assert!(CtlFlags::from_bits_retain(h.ctl(ADDR_CTL_FLAG)).contains(CtlFlags::FAILSAFE));

        h.cpu.request_config_reset();
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.cpu.config().failsafe_timeout, None);
        assert_eq!(h.telemetry().failsafe, 1);
        assert!(CtlFlags::from_bits_retain(h.ctl(ADDR_CTL_FLAG)).contains(CtlFlags::FAILSAFE));

        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.telemetry().failsafe, 1);
    }

    #[test]
    fn a_requested_reset_stops_a_silent_host_from_tripping_an_enabled_failsafe() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..DEFAULT
        }));
        h.port.host_idle_ms = Some(499);
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.telemetry().failsafe, 0);

        h.cpu.request_config_reset();
        h.port.host_idle_ms = Some(u32::MAX);
        h.cpu.tick_1ms(&mut h.port);
        assert_eq!(h.telemetry().failsafe, 0);
        assert!(!CtlFlags::from_bits_retain(h.ctl(ADDR_CTL_FLAG)).contains(CtlFlags::FAILSAFE));
    }
}
