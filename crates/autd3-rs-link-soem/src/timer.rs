#[cfg(target_os = "windows")]
mod imp {
    #[link(name = "winmm")]
    unsafe extern "system" {
        fn timeBeginPeriod(u_period: u32) -> u32;
        fn timeEndPeriod(u_period: u32) -> u32;
    }

    const TIMERR_NOERROR: u32 = 0;

    pub(crate) struct TimerResolutionGuard {
        period: Option<u32>,
    }

    impl TimerResolutionGuard {
        pub(crate) fn new(period: u32) -> Self {
            // SAFETY: `timeBeginPeriod` is a plain WinMM call with no
            // preconditions; the matching `timeEndPeriod` is issued in `Drop`.
            let active = unsafe { timeBeginPeriod(period) } == TIMERR_NOERROR;
            if active {
                tracing::debug!(period, "raised Windows timer resolution");
            } else {
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
                // SAFETY: balances the `timeBeginPeriod` from `new`.
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
