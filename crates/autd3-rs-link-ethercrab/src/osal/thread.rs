pub use autd3_rs_core::{CoreId, RtSchedulePolicy};
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};

#[derive(Clone, Copy, Default)]
pub(crate) struct PumpTuning {
    pub(crate) priority: Option<ThreadPriority>,
    pub(crate) policy: RtSchedulePolicy,
    pub(crate) affinity: Option<CoreId>,
}

pub(crate) fn apply_thread_tuning(
    priority: Option<ThreadPriority>,
    policy: RtSchedulePolicy,
    affinity: Option<CoreId>,
) {
    autd3_rs_core::apply_thread_tuning(autd3_rs_core::RtThreadTuning {
        priority,
        policy,
        affinity,
    });
}
