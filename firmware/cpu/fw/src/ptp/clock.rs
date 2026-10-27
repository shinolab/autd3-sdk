pub const NS_PER_SEC: u64 = 1_000_000_000;

const IMMEDIATE_STEP_LIMIT_NS: i64 = 500_000_000;
const IMMEDIATE_STEP_RESULT_MARGIN_NS: i64 = 10_000_000;
const SECOND_WRAP_GUARD_NS: u64 = 10_000;

#[derive(PartialEq, Eq, Debug)]
pub enum TimeStep {
    Add(i32),
    Rewrite(u64),
    Postpone,
    OutOfRange,
}

#[must_use]
pub const fn is_near_second_wrap(now_ns: u64) -> bool {
    now_ns % NS_PER_SEC >= NS_PER_SEC - SECOND_WRAP_GUARD_NS
}

#[must_use]
pub fn plan_step(now_ns: u64, offset_ns: i64) -> TimeStep {
    if is_near_second_wrap(now_ns) {
        return TimeStep::Postpone;
    }
    let within_second = now_ns % NS_PER_SEC;
    let Some(after) = within_second.cast_signed().checked_add(offset_ns) else {
        return TimeStep::OutOfRange;
    };
    if let Ok(immediate) = i32::try_from(offset_ns)
        && offset_ns.abs() < IMMEDIATE_STEP_LIMIT_NS
        && (IMMEDIATE_STEP_RESULT_MARGIN_NS
            ..NS_PER_SEC.cast_signed() - IMMEDIATE_STEP_RESULT_MARGIN_NS)
            .contains(&after)
    {
        return TimeStep::Add(immediate);
    }
    match now_ns.checked_add_signed(offset_ns) {
        Some(target) if u32::try_from(target / NS_PER_SEC).is_ok() => TimeStep::Rewrite(target),
        _ => TimeStep::OutOfRange,
    }
}

// This assumes a time of less than one second from recording to readout.
// In practice, it is called within a few ms such as inside a receive ISR or a 1 ms tick—this.
#[must_use]
pub fn complete(now_ns: u64, past_ns: u32) -> u64 {
    let sec = now_ns / NS_PER_SEC;
    let ns = now_ns % NS_PER_SEC;
    let sec = if u64::from(past_ns) <= ns {
        sec
    } else {
        sec.saturating_sub(1)
    };
    sec * NS_PER_SEC + u64::from(past_ns)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::{NS_PER_SEC, TimeStep, is_near_second_wrap, plan_step};

    const SEC: u64 = 7 * NS_PER_SEC;

    #[test]
    fn a_small_step_in_the_middle_of_a_second_is_added_in_place() {
        assert_eq!(
            TimeStep::Add(-20_000),
            plan_step(SEC + 500_000_000, -20_000)
        );
        assert_eq!(TimeStep::Add(20_000), plan_step(SEC + 500_000_000, 20_000));
    }

    #[test]
    fn a_backward_step_captured_just_before_the_wrap_is_postponed() {
        assert_eq!(
            TimeStep::Postpone,
            plan_step(SEC + 999_999_999, -20_000_000)
        );
        assert_eq!(
            TimeStep::Postpone,
            plan_step(SEC + 999_990_000, -20_000_000)
        );
        assert_eq!(
            TimeStep::Add(-20_000_000),
            plan_step(SEC + 999_989_999, -20_000_000)
        );
    }

    #[test]
    fn a_result_near_either_end_of_the_second_is_rewritten() {
        assert_eq!(
            TimeStep::Rewrite(SEC + 999_969_999),
            plan_step(SEC + 999_989_999, -20_000)
        );
        assert_eq!(
            TimeStep::Rewrite(SEC + 990_000_000),
            plan_step(SEC + 980_000_000, 10_000_000)
        );
        assert_eq!(
            TimeStep::Rewrite(SEC - 15_000),
            plan_step(SEC + 5_000, -20_000)
        );
        assert_eq!(
            TimeStep::Rewrite(SEC + 9_999_999),
            plan_step(SEC + 19_999_999, -10_000_000)
        );
        assert_eq!(
            TimeStep::Add(-10_000_000),
            plan_step(SEC + 20_000_000, -10_000_000)
        );
        assert_eq!(
            TimeStep::Add(10_000_000),
            plan_step(SEC + 979_999_999, 10_000_000)
        );
    }

    #[test]
    fn a_step_of_half_a_second_or_more_is_rewritten() {
        assert_eq!(
            TimeStep::Add(-499_999_999),
            plan_step(SEC + 600_000_000, -499_999_999)
        );
        assert_eq!(
            TimeStep::Rewrite(SEC + 100_000_000),
            plan_step(SEC + 600_000_000, -500_000_000)
        );
        assert_eq!(
            TimeStep::Rewrite(SEC + 3 * NS_PER_SEC + 999_989_999),
            plan_step(SEC + 999_989_999, 3 * NS_PER_SEC.cast_signed())
        );
    }

    #[test]
    fn a_rewrite_captured_just_before_the_wrap_is_postponed() {
        assert_eq!(TimeStep::Postpone, plan_step(SEC + 999_999_999, -20_000));
        assert_eq!(
            TimeStep::Postpone,
            plan_step(SEC + 999_990_000, 3 * NS_PER_SEC.cast_signed())
        );
        assert_eq!(
            TimeStep::Postpone,
            plan_step(999_995_000, -NS_PER_SEC.cast_signed())
        );
    }

    #[test]
    fn the_last_ten_microseconds_of_a_second_are_near_the_wrap() {
        assert!(!is_near_second_wrap(SEC));
        assert!(!is_near_second_wrap(SEC + 999_989_999));
        assert!(is_near_second_wrap(SEC + 999_990_000));
        assert!(is_near_second_wrap(SEC + 999_999_999));
    }

    #[rstest]
    #[case::largest_offset_mid_second(SEC + 500_000_000, i64::MAX)]
    #[case::largest_offset_late_in_the_second(SEC + 999_989_999, i64::MAX)]
    #[case::sum_one_past_the_signed_range(SEC + 500_000_000, i64::MAX - 499_999_999)]
    #[case::sum_at_the_end_of_the_signed_range(SEC + 500_000_000, i64::MAX - 500_000_000)]
    #[case::largest_offset_at_the_start_of_a_second(SEC, i64::MAX)]
    #[case::smallest_offset(SEC + 500_000_000, i64::MIN)]
    #[case::smallest_offset_at_the_start_of_time(0, i64::MIN)]
    #[case::smallest_offset_at_the_end_of_the_timer_range(
        u64::from(u32::MAX) * NS_PER_SEC + 500_000_000,
        i64::MIN
    )]
    fn an_offset_at_the_ends_of_the_signed_range_is_refused_without_wrapping(
        #[case] now_ns: u64,
        #[case] offset_ns: i64,
    ) {
        assert_eq!(TimeStep::OutOfRange, plan_step(now_ns, offset_ns));
    }

    #[test]
    fn a_target_outside_the_timer_range_is_refused() {
        assert_eq!(TimeStep::OutOfRange, plan_step(5_000, -20_000));
        assert_eq!(
            TimeStep::OutOfRange,
            plan_step(
                u64::from(u32::MAX) * NS_PER_SEC + 500_000_000,
                NS_PER_SEC.cast_signed()
            )
        );
    }
}
