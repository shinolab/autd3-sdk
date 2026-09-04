#[cfg(feature = "logging")]
mod logging;

mod executor;
pub mod oneshot;
mod semaphore;

pub use executor::{Executor, block_on};
pub use semaphore::{Acquire, Semaphore, SemaphorePermit};

use thread_priority::{ThreadPriority, ThreadPriorityValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RtPriority(ThreadPriority);

impl RtPriority {
    pub const MIN: Self = Self(ThreadPriority::Min);
    pub const MAX: Self = Self(ThreadPriority::Max);

    #[must_use]
    pub fn new(value: u8) -> Option<Self> {
        ThreadPriorityValue::try_from(value)
            .ok()
            .map(|v| Self(ThreadPriority::Crossplatform(v)))
    }

    #[must_use]
    pub fn value(self) -> Option<u8> {
        match self.0 {
            ThreadPriority::Crossplatform(v) => Some(u8::from(v)),
            _ => None,
        }
    }

    #[must_use]
    pub fn step_below(self) -> Option<Self> {
        Self::new(self.value()?.checked_sub(1)?)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CoreId {
    pub id: usize,
}

impl From<CoreId> for core_affinity::CoreId {
    fn from(v: CoreId) -> Self {
        core_affinity::CoreId { id: v.id }
    }
}

impl From<core_affinity::CoreId> for CoreId {
    fn from(v: core_affinity::CoreId) -> Self {
        CoreId { id: v.id }
    }
}

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
    pub priority: Option<RtPriority>,
    pub policy: RtSchedulePolicy,
    pub affinity: Option<CoreId>,
}

#[cfg(not(target_os = "windows"))]
const RT_THREAD_PRIORITY: u8 = 80;

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
        match set_rt_priority(priority.0, tuning.policy) {
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
        if core_affinity::set_for_current(core.into()) {
            applied.affinity = Some(core);
        } else {
            tracing::warn!("failed to pin RT thread to core {}", core.id);
        }
    }
    applied
}

#[must_use]
#[allow(clippy::unnecessary_wraps)]
pub fn default_rt_priority() -> Option<RtPriority> {
    #[cfg(target_os = "windows")]
    {
        Some(RtPriority(ThreadPriority::Os(
            thread_priority::WinAPIThreadPriority::TimeCritical.into(),
        )))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Some(RtPriority::new(RT_THREAD_PRIORITY).expect("0..=99 is a valid thread priority"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crossplatform(value: u8) -> RtPriority {
        RtPriority::new(value).unwrap()
    }

    #[test]
    fn a_step_below_is_one_lower() {
        assert_eq!(crossplatform(80).step_below(), Some(crossplatform(79)));
    }

    #[test]
    fn only_the_crossplatform_ladder_steps_down() {
        assert_eq!(RtPriority::MAX.step_below(), None);
        assert_eq!(RtPriority::MIN.step_below(), None);
        assert_eq!(crossplatform(0).step_below(), None);
    }

    #[test]
    fn only_the_crossplatform_ladder_has_a_value() {
        assert_eq!(crossplatform(80).value(), Some(80));
        assert_eq!(RtPriority::MAX.value(), None);
        assert_eq!(RtPriority::MIN.value(), None);
    }
}
