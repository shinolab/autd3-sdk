use crate::net::Mac;

pub const NS_PER_SEC: u64 = 1_000_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TxStamp {
    pub ns: u32,
    pub overwritten: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RxMeta {
    pub port: u8,
    pub timestamp_ns: u32,
}

const IMMEDIATE_STEP_LIMIT_NS: i64 = 500_000_000;
const IMMEDIATE_STEP_RESULT_MARGIN_NS: i64 = 10_000_000;
const IMMEDIATE_STEP_WRAP_GUARD_NS: u64 = 10_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimeStep {
    Add(i32),
    Rewrite(u64),
    Postpone,
    OutOfRange,
}

#[must_use]
pub fn plan_step(now_ns: u64, offset_ns: i64) -> TimeStep {
    let within_second = now_ns % NS_PER_SEC;
    let after = within_second.cast_signed() + offset_ns;
    if let Ok(immediate) = i32::try_from(offset_ns)
        && offset_ns.abs() < IMMEDIATE_STEP_LIMIT_NS
        && (IMMEDIATE_STEP_RESULT_MARGIN_NS
            ..NS_PER_SEC.cast_signed() - IMMEDIATE_STEP_RESULT_MARGIN_NS)
            .contains(&after)
    {
        if within_second >= NS_PER_SEC - IMMEDIATE_STEP_WRAP_GUARD_NS {
            return TimeStep::Postpone;
        }
        return TimeStep::Add(immediate);
    }
    match now_ns.checked_add_signed(offset_ns) {
        Some(target) if u32::try_from(target / NS_PER_SEC).is_ok() => TimeStep::Rewrite(target),
        _ => TimeStep::OutOfRange,
    }
}

pub trait Nic {
    fn send(&mut self, frame: &[u8], port: u8, timestamp: bool) -> bool;

    fn now(&mut self) -> Option<u64>;

    fn step(&mut self, offset_ns: i64) -> bool;

    fn set_time(&mut self, ns: u64) -> bool;

    fn set_drift(&mut self, ppb: i32);

    fn clear_tx_timestamps(&mut self);

    fn take_tx_timestamp(&mut self, port: u8) -> Option<TxStamp>;

    fn set_forwarding(&mut self, open: bool);

    fn set_mac(&mut self, mac: Mac);

    fn downstream_link(&mut self, port: u8) -> bool;

    fn arm_pulse(&mut self) -> bool;

    fn stop_pulse(&mut self);

    fn pulse_ready(&mut self) -> bool;
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

#[must_use]
pub const fn other_port(port: u8) -> u8 {
    port ^ 1
}

#[cfg(test)]
mod tests {
    use super::{NS_PER_SEC, TimeStep, plan_step};

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
            TimeStep::Rewrite(SEC + 999_979_999),
            plan_step(SEC + 999_999_999, -20_000)
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
            TimeStep::Rewrite(SEC + 3 * NS_PER_SEC + 999_999_999),
            plan_step(SEC + 999_999_999, 3 * NS_PER_SEC.cast_signed())
        );
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
