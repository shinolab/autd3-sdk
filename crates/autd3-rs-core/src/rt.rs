pub use core_affinity::CoreId;
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RtSchedulePolicy {
    Normal,
    #[default]
    Fifo,
    RoundRobin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RtThreadTuning {
    pub priority: Option<ThreadPriority>,
    pub policy: RtSchedulePolicy,
    pub affinity: Option<CoreId>,
}

#[cfg(not(target_os = "windows"))]
pub const RT_THREAD_PRIORITY: u8 = 80;

#[cfg(target_os = "linux")]
const RT_PRIORITY_REMEDY: &str = "Grant the capability with \
     `sudo setcap cap_sys_nice+ep <executable>`, or raise `rtprio` for the user in \
     /etc/security/limits.conf.";

#[cfg(not(target_os = "linux"))]
const RT_PRIORITY_REMEDY: &str = "Run with sufficient scheduling privileges.";

#[cfg(target_os = "linux")]
fn set_rt_priority(
    priority: ThreadPriority,
    policy: RtSchedulePolicy,
) -> Result<(), thread_priority::Error> {
    use thread_priority::{
        RealtimeThreadSchedulePolicy, ThreadSchedulePolicy, set_thread_priority_and_policy,
        thread_native_id,
    };
    let policy = match policy {
        RtSchedulePolicy::Normal => return thread_priority::set_current_thread_priority(priority),
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
fn set_rt_priority(
    priority: ThreadPriority,
    _policy: RtSchedulePolicy,
) -> Result<(), thread_priority::Error> {
    thread_priority::set_current_thread_priority(priority)
}

pub fn apply_thread_tuning(tuning: RtThreadTuning) {
    if let Some(priority) = tuning.priority {
        match set_rt_priority(priority, tuning.policy) {
            Ok(()) => {
                tracing::debug!(?priority, policy = ?tuning.policy, "applied RT thread scheduling");
            }
            Err(e) => tracing::warn!(
                "failed to set RT thread priority: {e:?}. The bus will be unstable under load. {}",
                RT_PRIORITY_REMEDY
            ),
        }
    }
    if let Some(core) = tuning.affinity
        && !core_affinity::set_for_current(core)
    {
        tracing::warn!("failed to pin RT thread to core {}", core.id);
    }
}

#[must_use]
#[allow(clippy::unnecessary_wraps)]
pub fn default_rt_priority() -> Option<ThreadPriority> {
    #[cfg(target_os = "windows")]
    {
        Some(ThreadPriority::Os(
            thread_priority::WinAPIThreadPriority::TimeCritical.into(),
        ))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Some(ThreadPriority::Crossplatform(
            ThreadPriorityValue::try_from(RT_THREAD_PRIORITY)
                .expect("0..=99 is a valid thread priority"),
        ))
    }
}
