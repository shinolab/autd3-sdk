use std::time::Duration;

#[must_use]
pub(crate) fn landing_target_ns(cycle_ns: u64, shift_ns: u64) -> u64 {
    if cycle_ns == 0 {
        return 0;
    }
    (shift_ns + cycle_ns / 2) % cycle_ns
}

#[must_use]
pub(crate) fn next_cycle_wait(
    dc_system_time_ns: u64,
    cycle: Duration,
    shift: Duration,
) -> Duration {
    let cycle_ns = u64::try_from(cycle.as_nanos()).unwrap_or(u64::MAX);
    if cycle_ns == 0 {
        return Duration::ZERO;
    }
    let shift_ns = u64::try_from(shift.as_nanos()).unwrap_or(u64::MAX);
    let phase = dc_system_time_ns % cycle_ns;
    Duration::from_nanos((cycle_ns - phase) + landing_target_ns(cycle_ns, shift_ns))
}

#[must_use]
pub(crate) fn phase_deviation_ns(dc_system_time_ns: u64, cycle: Duration, shift: Duration) -> u64 {
    let cycle_ns = u64::try_from(cycle.as_nanos()).unwrap_or(u64::MAX);
    if cycle_ns == 0 {
        return 0;
    }
    let shift_ns = u64::try_from(shift.as_nanos()).unwrap_or(u64::MAX);
    let target = landing_target_ns(cycle_ns, shift_ns);
    let diff = (dc_system_time_ns % cycle_ns + cycle_ns - target) % cycle_ns;
    diff.min(cycle_ns - diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CYCLE: Duration = Duration::from_millis(1);

    #[test]
    fn frames_land_in_the_middle_of_the_sync0_period() {
        assert_eq!(
            next_cycle_wait(0, CYCLE, Duration::ZERO),
            Duration::from_micros(1_500)
        );
        assert_eq!(
            next_cycle_wait(500_000, CYCLE, Duration::ZERO),
            Duration::from_millis(1)
        );
        assert_eq!(
            next_cycle_wait(42_500_000, CYCLE, Duration::ZERO),
            Duration::from_millis(1)
        );
    }

    #[test]
    fn the_sync0_shift_moves_the_landing_target() {
        assert_eq!(
            next_cycle_wait(0, CYCLE, Duration::from_micros(250)),
            Duration::from_micros(1_750)
        );
        assert_eq!(
            next_cycle_wait(0, CYCLE, CYCLE),
            next_cycle_wait(0, CYCLE, Duration::ZERO)
        );
    }

    #[test]
    fn the_deviation_is_measured_from_the_landing_target() {
        assert_eq!(phase_deviation_ns(500_000, CYCLE, Duration::ZERO), 0);
        assert_eq!(phase_deviation_ns(501_000, CYCLE, Duration::ZERO), 1_000);
        assert_eq!(phase_deviation_ns(0, CYCLE, Duration::ZERO), 500_000);
        assert_eq!(phase_deviation_ns(999_000, CYCLE, Duration::ZERO), 499_000);
    }

    #[test]
    fn a_zero_cycle_never_paces() {
        assert_eq!(
            next_cycle_wait(123, Duration::ZERO, Duration::ZERO),
            Duration::ZERO
        );
        assert_eq!(phase_deviation_ns(123, Duration::ZERO, Duration::ZERO), 0);
    }
}
