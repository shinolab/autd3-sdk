#[cfg(target_os = "windows")]
mod imp {
    use std::time::Duration;

    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(uPeriod: u32) -> u32;
        fn timeEndPeriod(uPeriod: u32) -> u32;
    }

    const TIMERR_NOERROR: u32 = 0;

    pub(crate) struct TimerResolutionGuard {
        period: Option<u32>,
    }

    impl TimerResolutionGuard {
        pub(crate) fn new(resolution: Option<Duration>) -> Self {
            let period = resolution.and_then(|resolution| {
                let period = u32::try_from(resolution.as_millis()).unwrap_or(u32::MAX);
                let active = unsafe { timeBeginPeriod(period) } == TIMERR_NOERROR;
                if !active {
                    tracing::warn!(period, "failed to raise Windows timer resolution");
                }
                active.then_some(period)
            });
            Self { period }
        }
    }

    impl Drop for TimerResolutionGuard {
        fn drop(&mut self) {
            if let Some(period) = self.period {
                unsafe { timeEndPeriod(period) };
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use std::time::Duration;

    pub(crate) struct TimerResolutionGuard;

    impl TimerResolutionGuard {
        pub(crate) fn new(_resolution: Option<Duration>) -> Self {
            Self
        }
    }
}

pub(crate) use imp::TimerResolutionGuard;
