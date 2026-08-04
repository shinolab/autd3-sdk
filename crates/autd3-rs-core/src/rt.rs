#[cfg(feature = "logging")]
mod logging;

pub use core_affinity::CoreId;
pub use thread_priority::{ThreadPriority, ThreadPriorityValue};

#[cfg(feature = "logging")]
pub use logging::{LogWriter, TracingGuard, TracingOption, init_tracing};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
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

pub fn apply_thread_tuning(tuning: RtThreadTuning) -> RtThreadTuning {
    let mut applied = RtThreadTuning {
        priority: None,
        policy: tuning.policy,
        affinity: None,
    };
    if let Some(priority) = tuning.priority {
        match set_rt_priority(priority, tuning.policy) {
            Ok(()) => {
                tracing::debug!(?priority, policy = ?tuning.policy, "applied RT thread scheduling");
                applied.priority = Some(priority);
            }
            Err(e) => tracing::warn!(
                "failed to set RT thread priority: {e:?}. The bus will be unstable under load. {}",
                RT_PRIORITY_REMEDY
            ),
        }
    }
    if let Some(core) = tuning.affinity {
        if core_affinity::set_for_current(core) {
            applied.affinity = Some(core);
        } else {
            tracing::warn!("failed to pin RT thread to core {}", core.id);
        }
    }
    applied
}

#[must_use]
pub fn step_below(priority: ThreadPriority) -> Option<ThreadPriority> {
    let ThreadPriority::Crossplatform(value) = priority else {
        return None;
    };
    let below = u8::from(value).checked_sub(1)?;
    ThreadPriorityValue::try_from(below)
        .ok()
        .map(ThreadPriority::Crossplatform)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn crossplatform(value: u8) -> ThreadPriority {
        ThreadPriority::Crossplatform(ThreadPriorityValue::try_from(value).unwrap())
    }

    #[test]
    fn a_step_below_is_one_lower() {
        assert_eq!(step_below(crossplatform(80)), Some(crossplatform(79)));
    }

    #[test]
    fn only_the_crossplatform_ladder_steps_down() {
        assert_eq!(step_below(ThreadPriority::Max), None);
        assert_eq!(step_below(ThreadPriority::Min), None);
        assert_eq!(step_below(crossplatform(0)), None);
    }
}
