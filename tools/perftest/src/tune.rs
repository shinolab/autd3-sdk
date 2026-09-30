use thread_priority::{ThreadPriority, ThreadPriorityValue};

use crate::cli::{Cli, RtPolicy};

#[derive(Clone, Copy, Debug)]
pub struct ThreadTuning {
    priority: Option<u8>,
    policy: RtPolicy,
    affinity: Option<usize>,
}

impl From<&Cli> for ThreadTuning {
    fn from(cli: &Cli) -> Self {
        Self {
            priority: cli.rt_priority,
            policy: cli.rt_policy,
            affinity: cli.rt_affinity,
        }
    }
}

#[cfg(target_os = "linux")]
fn set_priority(priority: ThreadPriority, policy: RtPolicy) -> Result<(), thread_priority::Error> {
    use thread_priority::{
        RealtimeThreadSchedulePolicy, ThreadSchedulePolicy, set_thread_priority_and_policy,
        thread_native_id,
    };
    let policy = match policy {
        RtPolicy::Normal => return thread_priority::set_current_thread_priority(priority),
        RtPolicy::Fifo => ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::Fifo),
        RtPolicy::RoundRobin => {
            ThreadSchedulePolicy::Realtime(RealtimeThreadSchedulePolicy::RoundRobin)
        }
    };
    set_thread_priority_and_policy(thread_native_id(), priority, policy)
}

#[cfg(not(target_os = "linux"))]
fn set_priority(priority: ThreadPriority, _policy: RtPolicy) -> Result<(), thread_priority::Error> {
    thread_priority::set_current_thread_priority(priority)
}

pub fn apply(tuning: ThreadTuning) {
    if let Some(value) = tuning.priority {
        let applied = ThreadPriorityValue::try_from(value)
            .map_err(|e| format!("{e:?}"))
            .and_then(|v| {
                set_priority(ThreadPriority::Crossplatform(v), tuning.policy)
                    .map_err(|e| format!("{e:?}"))
            });
        match applied {
            Ok(()) => eprintln!("driver thread: priority {value} ({:?})", tuning.policy),
            Err(e) => eprintln!("driver thread: failed to set priority {value}: {e}"),
        }
    }
    if let Some(id) = tuning.affinity {
        if core_affinity::set_for_current(core_affinity::CoreId { id }) {
            eprintln!("driver thread: pinned to core {id}");
        } else {
            eprintln!("driver thread: failed to pin to core {id}");
        }
    }
}
