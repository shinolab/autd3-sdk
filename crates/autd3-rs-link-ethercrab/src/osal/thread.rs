pub use autd3_rs_core::RtSchedulePolicy;
pub use core_affinity::CoreId;
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};

#[derive(Clone, Copy, Default)]
pub(crate) struct PumpTuning {
    pub(crate) priority: Option<ThreadPriority>,
    pub(crate) policy: RtSchedulePolicy,
    pub(crate) affinity: Option<CoreId>,
}

#[cfg(target_os = "linux")]
fn set_thread_priority(
    priority: ThreadPriority,
    policy: RtSchedulePolicy,
) -> Result<(), thread_priority::Error> {
    use thread_priority::{
        RealtimeThreadSchedulePolicy, ThreadSchedulePolicy, set_thread_priority_and_policy,
        thread_native_id,
    };
    let policy = match policy {
        RtSchedulePolicy::Normal => {
            return thread_priority::set_current_thread_priority(priority);
        }
        RtSchedulePolicy::Fifo => {
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo)
        }
        RtSchedulePolicy::RoundRobin => {
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::RoundRobin)
        }
    };
    set_thread_priority_and_policy(thread_native_id(), priority, policy)
}

#[cfg(not(target_os = "linux"))]
fn set_thread_priority(
    priority: ThreadPriority,
    _policy: RtSchedulePolicy,
) -> Result<(), thread_priority::Error> {
    thread_priority::set_current_thread_priority(priority)
}

pub(crate) fn apply_thread_tuning(
    priority: Option<ThreadPriority>,
    policy: RtSchedulePolicy,
    affinity: Option<CoreId>,
) {
    if let Some(priority) = priority {
        match set_thread_priority(priority, policy) {
            Ok(()) => tracing::debug!(?priority, ?policy, "applied tx/rx RT thread scheduling"),
            Err(e) => tracing::warn!("failed to set tx/rx thread priority: {e:?}"),
        }
    }
    if let Some(core) = affinity
        && !core_affinity::set_for_current(core)
    {
        tracing::warn!("failed to pin tx/rx thread to core {}", core.id);
    }
}
