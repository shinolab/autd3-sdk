use core::time::Duration;

use crate::Cpu;
use crate::cmd::cpu_config::CpuConfig;
use crate::fpga;
use crate::fpga_params::{ADDR_CTL_FLAG, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

fn expired(elapsed_ms: Option<u32>, timeout: Option<Duration>) -> bool {
    timeout
        .is_some_and(|timeout| elapsed_ms.is_some_and(|ms| u128::from(ms) >= timeout.as_millis()))
}

pub(crate) fn host_silent(cpu: &Cpu, config: &CpuConfig) -> bool {
    expired(cpu.host_idle_ms(), config.failsafe_timeout)
}

pub(crate) fn ptp_unlock_expired(cpu: &Cpu, config: &CpuConfig) -> bool {
    expired(cpu.ptp_unlocked_ms(), config.ptp_unlock_failsafe_timeout)
}

fn set<P: Port>(port: &mut P, active: bool) {
    let mut ctl = CtlFlags::from_bits_retain(fpga::read_ctl(port, ADDR_CTL_FLAG));
    ctl.set(CtlFlags::FAILSAFE, active);
    fpga::write_ctl(port, ADDR_CTL_FLAG, ctl.bits());
}

pub(crate) fn mute<P: Port>(port: &mut P) {
    set(port, true);
}

pub(crate) fn release<P: Port>(port: &mut P, cpu: &Cpu, config: &CpuConfig) -> Result<(), Error> {
    if host_silent(cpu, config) || ptp_unlock_expired(cpu, config) {
        return Err(Error::FailsafeConditionActive);
    }
    set(port, false);
    fpga::set_watchdog_timeout(port, config.cpu_watchdog_timeout);
    fpga::release_watchdog(port);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;

    use core::time::Duration;

    use autd3_cpu_wire::cpu_params;

    use crate::cmd::cpu_config::CpuConfig;
    use crate::fpga_params::{CtlFlags, NUM_TRANSDUCERS};
    use crate::proto::{Cmd, Error};
    use crate::test_utils::builders::{force_fan, output_mask, set_cpu_config};
    use crate::test_utils::mock::{Frame, Harness, WatchdogEvent};

    const FAILSAFE_TIMEOUT_MS: u32 = 500;
    const FAILSAFE_TIMEOUT: Duration = Duration::from_millis(FAILSAFE_TIMEOUT_MS as u64);

    fn armed() -> Harness {
        let mut h = Harness::new();
        h.cpu.apply_config(
            &mut h.port,
            CpuConfig {
                failsafe_timeout: Some(FAILSAFE_TIMEOUT),
                ..CpuConfig::DEFAULT
            },
        );
        h
    }

    fn set_failsafe_timeout(h: &mut Harness, failsafe_timeout: Option<Duration>) {
        let config = h.cpu.config();
        h.deliver(&set_cpu_config(&CpuConfig {
            failsafe_timeout,
            ..config
        }));
        assert_eq!(h.status(), Error::None);
    }

    fn set_mask(h: &mut Harness) {
        let mask = vec![true; NUM_TRANSDUCERS];
        h.deliver(&output_mask(&mask));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(h);
    }

    fn release(h: &mut Harness) {
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(h);
    }

    fn assert_mask_kept(h: &Harness) {
        assert_eq!(h.output_mask(0), 0xFFFF);
    }

    fn assert_muted(h: &Harness) {
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_mask_kept(h);
    }

    fn assert_not_muted(h: &Harness) {
        assert!(!h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_mask_kept(h);
    }

    #[test]
    fn failsafe_trips_once_the_host_is_silent_for_the_timeout() {
        let mut h = armed();
        set_mask(&mut h);

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS - 1);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().failsafe, 1);

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS * 3);
        h.tick_1ms(10);
        assert_eq!(h.telemetry().failsafe, 1);
    }

    #[test]
    fn failsafe_never_trips_before_the_host_is_seen() {
        let mut h = armed();
        set_mask(&mut h);

        h.tick_1ms(FAILSAFE_TIMEOUT_MS * 2);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);
    }

    #[test]
    fn failsafe_rearms_after_the_host_comes_back() {
        let mut h = armed();
        set_mask(&mut h);

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.telemetry().failsafe, 1);

        h.set_host_idle_ms(3);
        h.tick_1ms(1);
        release(&mut h);
        assert_not_muted(&h);

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS + 7);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().failsafe, 2);
    }

    #[test]
    fn failsafe_follows_the_configured_timeout() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));

        h.set_host_idle_ms(49);
        h.tick_1ms(1);
        assert_not_muted(&h);

        h.set_host_idle_ms(50);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().failsafe, 1);
    }

    #[test]
    fn a_long_timeout_is_honoured() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_failsafe_timeout(&mut h, Some(Duration::from_secs(2)));

        h.set_host_idle_ms(1999);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);

        h.set_host_idle_ms(2000);
        h.tick_1ms(1);
        assert_muted(&h);
    }

    #[test]
    fn the_host_failsafe_is_disabled_by_default() {
        let mut h = Harness::new();
        set_mask(&mut h);
        assert_eq!(h.cpu.config().failsafe_timeout, None);

        h.set_host_idle_ms(u32::MAX);
        h.tick_1ms(10);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);
    }

    #[test]
    fn a_disabled_failsafe_never_trips() {
        let mut h = armed();
        set_mask(&mut h);
        set_failsafe_timeout(&mut h, None);

        h.set_host_idle_ms(u32::MAX);
        h.tick_1ms(10);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);
    }

    #[test]
    fn disabling_after_a_trip_does_not_unmute_and_reenabling_trips_again() {
        let mut h = armed();
        set_mask(&mut h);
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        set_failsafe_timeout(&mut h, None);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().failsafe, 1);

        release(&mut h);
        set_failsafe_timeout(&mut h, Some(FAILSAFE_TIMEOUT));
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().failsafe, 2);
    }

    #[test]
    fn clear_disables_an_enabled_failsafe() {
        let mut h = armed();
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(&h);
        assert_eq!(h.cpu.config().failsafe_timeout, None);

        h.set_host_idle_ms(u32::MAX);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);
    }

    fn set_ptp_unlock_failsafe_timeout(h: &mut Harness, timeout: Option<Duration>) {
        let config = h.cpu.config();
        h.deliver(&set_cpu_config(&CpuConfig {
            ptp_unlock_failsafe_timeout: timeout,
            ..config
        }));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn a_lost_ptp_lock_never_trips_the_failsafe_by_default() {
        let mut h = Harness::new();
        set_mask(&mut h);
        h.cpu.publish_ptp_unlocked(Some(u32::MAX));
        h.tick_1ms(3);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);
    }

    #[test]
    fn failsafe_trips_once_the_ptp_lock_is_lost_for_the_timeout() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_secs(2)));

        h.cpu.publish_ptp_unlocked(Some(1999));
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);

        h.cpu.publish_ptp_unlocked(Some(2000));
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);
        assert_eq!(h.telemetry().failsafe, 0);

        h.cpu.publish_ptp_unlocked(Some(6000));
        h.tick_1ms(1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);
    }

    #[test]
    fn the_ptp_unlock_failsafe_trips_while_the_fifo_never_empties() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.deliver_no_drain(&Frame::new(Cmd::Nop));

        for unlocked_ms in 0..50 {
            h.cpu.publish_ptp_unlocked(Some(unlocked_ms));
            h.step_backlogged(1);
        }
        assert_not_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);

        h.cpu.publish_ptp_unlocked(Some(50));
        h.step_backlogged(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);
        assert_eq!(h.telemetry().failsafe, 0);

        h.cpu.publish_ptp_unlocked(Some(51));
        h.step_backlogged(1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);
        assert!(h.process_one());
    }

    #[test]
    fn the_host_failsafe_stays_quiet_while_the_fifo_never_empties() {
        let mut h = armed();
        set_mask(&mut h);
        h.deliver_no_drain(&Frame::new(Cmd::Nop));

        h.set_host_idle_ms(0);
        for _ in 0..FAILSAFE_TIMEOUT_MS + 10 {
            h.step_backlogged(1);
        }
        assert_not_muted(&h);
        assert_eq!(h.telemetry().failsafe, 0);
    }

    #[test]
    fn the_ptp_unlock_failsafe_never_trips_while_locked_or_before_the_first_lock() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(1)));
        h.cpu.publish_ptp_unlocked(None);
        h.tick_1ms(5000);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);
    }

    #[test]
    fn the_ptp_unlock_failsafe_rearms_after_a_relock_and_stays_muted() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);

        h.cpu.publish_ptp_unlocked(None);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);

        release(&mut h);
        h.cpu.publish_ptp_unlocked(Some(51));
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 2);
    }

    #[test]
    fn the_host_and_ptp_failsafes_trip_independently() {
        let mut h = armed();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.telemetry().failsafe, 1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);

        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert_eq!(h.telemetry().failsafe, 1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);
    }

    #[test]
    fn clear_disables_the_ptp_unlock_failsafe_again() {
        let mut h = Harness::new();
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 0);
    }

    #[test]
    fn the_failsafe_leaves_the_output_mask_as_the_user_wrote_it() {
        let mut h = armed();
        let mask: std::vec::Vec<bool> = (0..NUM_TRANSDUCERS).map(|i| i % 3 == 0).collect();
        h.deliver(&output_mask(&mask));
        let before = h.port.output_mask.clone();

        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_eq!(h.port.output_mask, before);

        h.set_host_idle_ms(0);
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::None);
        assert!(!h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_eq!(h.port.output_mask, before);
    }

    #[test]
    fn neither_the_host_coming_back_nor_set_output_mask_releases_the_failsafe() {
        let mut h = armed();
        set_mask(&mut h);
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        h.set_host_idle_ms(0);
        h.tick_1ms(10);
        assert_muted(&h);

        h.deliver(&output_mask(&vec![true; NUM_TRANSDUCERS]));
        assert_eq!(h.status(), Error::None);
        assert_muted(&h);
    }

    #[test]
    fn clear_releases_the_failsafe() {
        let mut h = armed();
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));

        h.set_host_idle_ms(0);
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(&h);
    }

    #[test]
    fn releasing_without_a_trip_changes_nothing() {
        let mut h = Harness::new();
        h.deliver(&force_fan(1));
        let before = h.ctl_flags();
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl_flags(), before);
    }

    #[test]
    fn the_failsafe_and_its_release_keep_the_other_persistent_flags() {
        let mut h = armed();
        h.deliver(&force_fan(1));
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.ctl_flags(), CtlFlags::FORCE_FAN | CtlFlags::FAILSAFE);

        h.set_host_idle_ms(0);
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.ctl_flags(), CtlFlags::FORCE_FAN);
    }

    #[test]
    fn a_release_is_refused_while_the_ptp_lock_is_still_lost() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert_muted(&h);

        h.cpu.publish_ptp_unlocked(Some(500));
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert_muted(&h);
        assert_eq!(h.telemetry().ptp_unlock_failsafe, 1);

        h.cpu.publish_ptp_unlocked(None);
        release(&mut h);
    }

    #[test]
    fn the_ptp_failsafe_flag_rises_on_a_ptp_trip_and_stays_until_a_release() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(49));
        h.tick_1ms(1);
        assert!(!h.cpu.ptp_failsafe_active());

        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert!(h.cpu.ptp_failsafe_active());

        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert!(h.cpu.ptp_failsafe_active());

        h.cpu.publish_ptp_unlocked(None);
        h.tick_1ms(1);
        assert!(h.cpu.ptp_failsafe_active());

        release(&mut h);
        assert!(!h.cpu.ptp_failsafe_active());
    }

    #[test]
    fn the_ptp_failsafe_flag_rises_even_when_the_host_failsafe_already_muted() {
        let mut h = armed();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);
        assert!(!h.cpu.ptp_failsafe_active());

        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert!(h.cpu.ptp_failsafe_active());
    }

    #[test]
    fn a_host_failsafe_never_raises_the_ptp_failsafe_flag() {
        let mut h = armed();
        set_mask(&mut h);
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);
        assert!(!h.cpu.ptp_failsafe_active());
    }

    #[test]
    fn a_release_is_refused_while_the_host_still_counts_as_silent() {
        let mut h = armed();
        set_mask(&mut h);
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);

        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert_muted(&h);

        h.set_host_idle_ms(0);
        release(&mut h);
    }

    #[test]
    fn a_release_is_accepted_once_the_tripped_failsafe_is_disabled() {
        let mut h = Harness::new();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        assert_muted(&h);

        set_ptp_unlock_failsafe_timeout(&mut h, None);
        release(&mut h);
    }

    #[test]
    fn a_release_ignores_a_ptp_unlock_shorter_than_the_timeout() {
        let mut h = armed();
        set_mask(&mut h);
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        h.set_host_idle_ms(0);
        h.cpu.publish_ptp_unlocked(Some(49));
        release(&mut h);
    }

    const CPU_WATCHDOG_TIMEOUT_MS: u16 = cpu_params::CPU_WATCHDOG_TIMEOUT.as_millis() as u16;

    const _: () = assert!(CPU_WATCHDOG_TIMEOUT_MS == 1000);

    #[test]
    fn boot_releases_the_watchdog_and_then_arms_it_with_the_default_timeout() {
        let h = Harness::new();
        assert_eq!(
            h.port.watchdog_log,
            [
                WatchdogEvent::Release,
                WatchdogEvent::Timeout(CPU_WATCHDOG_TIMEOUT_MS)
            ]
        );
    }

    #[test]
    fn a_boot_whose_fpga_init_times_out_leaves_the_watchdog_fired() {
        let mut h = Harness::new();
        h.port.watchdog_log.clear();
        h.port.watchdog_state = 1;
        h.port.latch_stuck = true;
        h.reboot();
        assert_eq!(
            h.port.watchdog_log,
            [WatchdogEvent::Timeout(CPU_WATCHDOG_TIMEOUT_MS)]
        );
        assert_eq!(h.port.watchdog_state & 1, 1);
    }

    #[test]
    fn every_tick_kicks_the_watchdog() {
        let mut h = Harness::new();
        let kicks = h.port.watchdog_kicks;
        h.tick_1ms(3);
        assert_eq!(h.port.watchdog_kicks - kicks, 3);
    }

    #[test]
    fn the_watchdog_timeout_follows_the_config() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            cpu_watchdog_timeout: Some(Duration::from_millis(250)),
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.watchdog_timeout, Some(250));

        h.deliver(&set_cpu_config(&CpuConfig {
            cpu_watchdog_timeout: None,
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.watchdog_timeout, Some(0));

        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.port.watchdog_timeout, Some(CPU_WATCHDOG_TIMEOUT_MS));
    }

    #[test]
    fn a_config_reset_rearms_the_watchdog_with_the_default_timeout() {
        let mut h = Harness::new();
        h.deliver(&set_cpu_config(&CpuConfig {
            cpu_watchdog_timeout: None,
            ..CpuConfig::DEFAULT
        }));
        assert_eq!(h.port.watchdog_timeout, Some(0));

        h.cpu.request_config_reset();
        h.tick_1ms(1);
        assert_eq!(h.port.watchdog_timeout, Some(CPU_WATCHDOG_TIMEOUT_MS));
    }

    #[test]
    fn clear_releases_a_fired_watchdog_after_the_timeout_is_rewritten() {
        let mut h = Harness::new();
        h.port.watchdog_state = 1;
        h.port.watchdog_log.clear();
        h.deliver(&Frame::new(Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(
            h.port.watchdog_log,
            [
                WatchdogEvent::Timeout(CPU_WATCHDOG_TIMEOUT_MS),
                WatchdogEvent::Release
            ]
        );
        assert_eq!(h.port.watchdog_state & 1, 0);
    }

    #[test]
    fn release_failsafe_releases_a_fired_watchdog() {
        let mut h = Harness::new();
        h.port.watchdog_state = 1;
        let releases = h.port.watchdog_releases;
        release(&mut h);
        assert_eq!(h.port.watchdog_releases - releases, 1);
        assert_eq!(h.port.watchdog_state & 1, 0);
    }

    #[test]
    fn a_rejected_release_keeps_the_watchdog_fired() {
        let mut h = Harness::new();
        set_ptp_unlock_failsafe_timeout(&mut h, Some(Duration::from_millis(50)));
        h.cpu.publish_ptp_unlocked(Some(50));
        h.tick_1ms(1);
        h.port.watchdog_state = 1;
        let releases = h.port.watchdog_releases;
        h.deliver(&Frame::new(Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert_eq!(h.port.watchdog_releases, releases);
        assert_eq!(h.port.watchdog_state & 1, 1);
    }

    #[test]
    fn a_latch_keeps_the_failsafe() {
        let mut h = armed();
        h.set_host_idle_ms(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        h.set_host_idle_ms(0);
        h.deliver(&Frame::new(Cmd::Synchronize));
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));
    }
}
