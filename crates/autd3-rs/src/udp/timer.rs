#[cfg(target_os = "windows")]
mod imp {
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
        pub(crate) fn new(period: u32) -> Self {
            let active = unsafe { timeBeginPeriod(period) } == TIMERR_NOERROR;
            if !active {
                tracing::warn!(period, "failed to raise Windows timer resolution");
            }
            Self {
                period: active.then_some(period),
            }
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
    pub(crate) struct TimerResolutionGuard;

    impl TimerResolutionGuard {
        pub(crate) fn new(_period: u32) -> Self {
            Self
        }
    }
}

pub(crate) use imp::TimerResolutionGuard;
