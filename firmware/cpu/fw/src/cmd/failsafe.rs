use crate::cmd::cpu_config::CpuConfig;
use crate::fpga;
use crate::fpga_params::{ADDR_CTL_FLAG, CtlFlags};
use crate::port::Port;
use crate::proto::Error;

pub(crate) fn host_silent<P: Port>(port: &mut P, config: &CpuConfig) -> bool {
    config.failsafe_timeout.is_some_and(|timeout| {
        port.host_idle_ms()
            .is_some_and(|ms| u128::from(ms) >= timeout.as_millis())
    })
}

pub(crate) fn ptp_unlock_expired<P: Port>(port: &mut P, config: &CpuConfig) -> bool {
    config.ptp_unlock_failsafe_timeout.is_some_and(|timeout| {
        port.ptp_unlocked_ms()
            .is_some_and(|ms| u128::from(ms) >= timeout.as_millis())
    })
}

fn set<P: Port>(port: &mut P, active: bool) {
    let mut ctl = CtlFlags::from_bits_retain(fpga::read_ctl(port, ADDR_CTL_FLAG));
    ctl.set(CtlFlags::FAILSAFE, active);
    fpga::write_ctl(port, ADDR_CTL_FLAG, ctl.bits());
}

pub(crate) fn mute<P: Port>(port: &mut P) {
    set(port, true);
}

pub(crate) fn release<P: Port>(port: &mut P, config: &CpuConfig) -> Result<(), Error> {
    if host_silent(port, config) || ptp_unlock_expired(port, config) {
        return Err(Error::FailsafeConditionActive);
    }
    set(port, false);
    Ok(())
}

#[cfg(all(test, not(loom)))]
mod tests {
    use std::vec;

    use core::time::Duration;

    use autd3_cpu_wire::cpu_params;

    use crate::cmd::cpu_config::{CpuConfig, default_config};
    use crate::fpga_params::{CtlFlags, NUM_TRANSDUCERS};
    use crate::proto::{Cmd, Error, Telemetry};
    use crate::test_utils::builders::{force_fan, output_mask, set_cpu_config};
    use crate::test_utils::mock::{Frame, Harness};

    const FAILSAFE_TIMEOUT_MS: u32 = cpu_params::FAILSAFE_TIMEOUT.as_millis() as u32;

    const _: () = assert!(FAILSAFE_TIMEOUT_MS == 500);

    fn set_failsafe_timeout(h: &mut Harness, seq: u8, failsafe_timeout: Option<Duration>) {
        h.deliver(&set_cpu_config(
            seq,
            &CpuConfig {
                failsafe_timeout,
                ..default_config()
            },
        ));
        assert_eq!(h.status(), Error::None);
    }

    fn set_mask(h: &mut Harness, seq: u8) {
        let mask = vec![true; NUM_TRANSDUCERS];
        h.deliver(&output_mask(seq, &mask));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(h);
    }

    fn release(h: &mut Harness, seq: u8) {
        h.deliver(&Frame::new(seq, Cmd::ReleaseFailsafe));
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
        let mut h = Harness::new();
        set_mask(&mut h, 0);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS - 1);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS * 3);
        h.tick_1ms(10);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
    }

    #[test]
    fn failsafe_never_trips_before_the_host_is_seen() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        h.port.host_idle_ms = None;

        h.tick_1ms(FAILSAFE_TIMEOUT_MS * 2);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 0);
    }

    #[test]
    fn failsafe_rearms_after_the_host_comes_back() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

        h.port.host_idle_ms = Some(3);
        h.tick_1ms(1);
        release(&mut h, 1);
        assert_not_muted(&h);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS + 7);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 2);
    }

    #[test]
    fn failsafe_follows_the_configured_timeout() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));

        h.port.host_idle_ms = Some(49);
        h.tick_1ms(1);
        assert_not_muted(&h);

        h.port.host_idle_ms = Some(50);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
    }

    #[test]
    fn a_longer_timeout_outlasts_the_default() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_failsafe_timeout(&mut h, 1, Some(Duration::from_secs(2)));

        h.port.host_idle_ms = Some(1999);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

        h.port.host_idle_ms = Some(2000);
        h.tick_1ms(1);
        assert_muted(&h);
    }

    #[test]
    fn a_disabled_failsafe_never_trips() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_failsafe_timeout(&mut h, 1, None);

        h.port.host_idle_ms = Some(u32::MAX);
        h.tick_1ms(10);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 0);
    }

    #[test]
    fn disabling_after_a_trip_does_not_unmute_and_reenabling_trips_again() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        set_failsafe_timeout(&mut h, 1, None);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);

        release(&mut h, 2);
        set_failsafe_timeout(&mut h, 3, Some(cpu_params::FAILSAFE_TIMEOUT));
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 2);
    }

    #[test]
    fn clear_reenables_a_disabled_failsafe() {
        let mut h = Harness::new();
        set_failsafe_timeout(&mut h, 0, None);
        h.deliver(&Frame::new(1, Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(&h);

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
    }

    fn set_ptp_unlock_failsafe_timeout(h: &mut Harness, seq: u8, timeout: Option<Duration>) {
        h.deliver(&set_cpu_config(
            seq,
            &CpuConfig {
                ptp_unlock_failsafe_timeout: timeout,
                ..default_config()
            },
        ));
        assert_eq!(h.status(), Error::None);
    }

    #[test]
    fn a_lost_ptp_lock_never_trips_the_failsafe_by_default() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        h.port.ptp_unlocked_ms = Some(u32::MAX);
        h.tick_1ms(3);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);
    }

    #[test]
    fn failsafe_trips_once_the_ptp_lock_is_lost_for_the_timeout() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_secs(2)));

        h.port.ptp_unlocked_ms = Some(1999);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);

        h.port.ptp_unlocked_ms = Some(2000);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 0);

        h.port.ptp_unlocked_ms = Some(6000);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);
    }

    #[test]
    fn the_ptp_unlock_failsafe_never_trips_while_locked_or_before_the_first_lock() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(1)));
        h.port.ptp_unlocked_ms = None;
        h.tick_1ms(5000);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);
    }

    #[test]
    fn the_ptp_unlock_failsafe_rearms_after_a_relock_and_stays_muted() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));
        h.port.ptp_unlocked_ms = Some(50);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);

        h.port.ptp_unlocked_ms = None;
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);

        release(&mut h, 2);
        h.port.ptp_unlocked_ms = Some(51);
        h.tick_1ms(1);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 2);
    }

    #[test]
    fn the_host_and_ptp_failsafes_trip_independently() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);

        h.port.ptp_unlocked_ms = Some(50);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::Failsafe), 1);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);
    }

    #[test]
    fn clear_disables_the_ptp_unlock_failsafe_again() {
        let mut h = Harness::new();
        set_ptp_unlock_failsafe_timeout(&mut h, 0, Some(Duration::from_millis(50)));
        h.port.ptp_unlocked_ms = Some(50);
        h.tick_1ms(1);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);

        h.deliver(&Frame::new(1, Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);
        h.tick_1ms(1);
        assert_not_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 0);
    }

    #[test]
    fn the_failsafe_leaves_the_output_mask_as_the_user_wrote_it() {
        let mut h = Harness::new();
        let mask: std::vec::Vec<bool> = (0..NUM_TRANSDUCERS).map(|i| i % 3 == 0).collect();
        h.deliver(&output_mask(0, &mask));
        let before = h.port.output_mask.clone();

        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_eq!(h.port.output_mask, before);

        h.port.host_idle_ms = Some(0);
        h.deliver(&Frame::new(1, Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::None);
        assert!(!h.ctl_flags().contains(CtlFlags::FAILSAFE));
        assert_eq!(h.port.output_mask, before);
    }

    #[test]
    fn neither_the_host_coming_back_nor_set_output_mask_releases_the_failsafe() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        h.port.host_idle_ms = Some(0);
        h.tick_1ms(10);
        assert_muted(&h);

        h.deliver(&output_mask(1, &vec![true; NUM_TRANSDUCERS]));
        assert_eq!(h.status(), Error::None);
        assert_muted(&h);
    }

    #[test]
    fn clear_releases_the_failsafe() {
        let mut h = Harness::new();
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));

        h.port.host_idle_ms = Some(0);
        h.deliver(&Frame::new(0, Cmd::Clear));
        assert_eq!(h.status(), Error::None);
        assert_not_muted(&h);
    }

    #[test]
    fn releasing_without_a_trip_changes_nothing() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 1));
        let before = h.ctl_flags();
        h.deliver(&Frame::new(1, Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::None);
        assert_eq!(h.ctl_flags(), before);
    }

    #[test]
    fn the_failsafe_and_its_release_keep_the_other_persistent_flags() {
        let mut h = Harness::new();
        h.deliver(&force_fan(0, 1));
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_eq!(h.ctl_flags(), CtlFlags::FORCE_FAN | CtlFlags::FAILSAFE);

        h.port.host_idle_ms = Some(0);
        h.deliver(&Frame::new(1, Cmd::ReleaseFailsafe));
        assert_eq!(h.ctl_flags(), CtlFlags::FORCE_FAN);
    }

    #[test]
    fn a_release_is_refused_while_the_ptp_lock_is_still_lost() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));
        h.port.ptp_unlocked_ms = Some(50);
        h.tick_1ms(1);
        assert_muted(&h);

        h.port.ptp_unlocked_ms = Some(500);
        h.deliver(&Frame::new(2, Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert_muted(&h);
        assert_eq!(h.telemetry(Telemetry::PtpUnlockFailsafe), 1);

        h.port.ptp_unlocked_ms = None;
        release(&mut h, 3);
    }

    #[test]
    fn a_release_is_refused_while_the_host_still_counts_as_silent() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);

        h.deliver(&Frame::new(1, Cmd::ReleaseFailsafe));
        assert_eq!(h.status(), Error::FailsafeConditionActive);
        assert_muted(&h);

        h.port.host_idle_ms = Some(0);
        release(&mut h, 2);
    }

    #[test]
    fn a_release_is_accepted_once_the_tripped_failsafe_is_disabled() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));
        h.port.ptp_unlocked_ms = Some(50);
        h.tick_1ms(1);
        assert_muted(&h);

        set_ptp_unlock_failsafe_timeout(&mut h, 2, None);
        release(&mut h, 3);
    }

    #[test]
    fn a_release_ignores_a_ptp_unlock_shorter_than_the_timeout() {
        let mut h = Harness::new();
        set_mask(&mut h, 0);
        set_ptp_unlock_failsafe_timeout(&mut h, 1, Some(Duration::from_millis(50)));
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        assert_muted(&h);

        h.port.host_idle_ms = Some(0);
        h.port.ptp_unlocked_ms = Some(49);
        release(&mut h, 2);
    }

    #[test]
    fn a_latch_keeps_the_failsafe() {
        let mut h = Harness::new();
        h.port.host_idle_ms = Some(FAILSAFE_TIMEOUT_MS);
        h.tick_1ms(1);
        h.port.host_idle_ms = Some(0);
        h.deliver(&Frame::new(0, Cmd::Synchronize));
        assert!(h.ctl_flags().contains(CtlFlags::FAILSAFE));
    }
}
