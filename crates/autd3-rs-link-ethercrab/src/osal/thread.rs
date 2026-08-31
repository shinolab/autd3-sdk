pub use autd3_rs_core::{CoreId, RtPriority, RtSchedulePolicy};

#[derive(Clone, Copy, Default)]
pub(crate) struct PumpTuning {
    pub(crate) priority: Option<RtPriority>,
    pub(crate) policy: RtSchedulePolicy,
    pub(crate) affinity: Option<CoreId>,
}

pub(crate) fn apply_thread_tuning(
    priority: Option<RtPriority>,
    policy: RtSchedulePolicy,
    affinity: Option<CoreId>,
) {
    autd3_rs_core::apply_thread_tuning(autd3_rs_core::RtThreadTuning {
        priority,
        policy,
        affinity,
    });
}
