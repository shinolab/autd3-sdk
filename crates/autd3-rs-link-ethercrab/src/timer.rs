use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
mod imp {
    use super::{Duration, Instant};

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

    pub(crate) async fn async_sleep(duration: Duration) {
        async_sleep_until(Instant::now() + duration).await;
    }

    pub(crate) async fn async_sleep_until(deadline: Instant) {
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;
            if remaining > Duration::from_micros(50) {
                std::thread::sleep(remaining - Duration::from_micros(50));
            } else {
                std::hint::spin_loop();
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::{Duration, Instant};

    pub(crate) struct TimerResolutionGuard;

    impl TimerResolutionGuard {
        pub(crate) fn new(_period: u32) -> Self {
            Self
        }
    }

    pub(crate) async fn async_sleep(duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    pub(crate) async fn async_sleep_until(deadline: Instant) {
        tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    }
}

pub(crate) use imp::TimerResolutionGuard;
pub(crate) use imp::{async_sleep, async_sleep_until};
