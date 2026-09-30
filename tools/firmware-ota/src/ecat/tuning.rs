#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, HIGH_PRIORITY_CLASS, SetPriorityClass,
    };

    const TIMER_PERIOD_MS: u32 = 1;
    const TIMERR_NOERROR: u32 = 0;

    pub struct PerfTuning {
        timer_set: bool,
    }

    impl PerfTuning {
        #[must_use]
        pub fn apply() -> Self {
            // SAFETY: timeBeginPeriod is a thread-safe winmm call; it is paired
            // with timeEndPeriod(TIMER_PERIOD_MS) in Drop.
            let timer_set = unsafe { timeBeginPeriod(TIMER_PERIOD_MS) } == TIMERR_NOERROR;
            // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no
            // close; SetPriorityClass only reads it.
            unsafe {
                SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS);
            }
            Self { timer_set }
        }
    }

    impl Drop for PerfTuning {
        fn drop(&mut self) {
            if self.timer_set {
                // SAFETY: matches the earlier timeBeginPeriod(TIMER_PERIOD_MS).
                unsafe {
                    timeEndPeriod(TIMER_PERIOD_MS);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct PerfTuning;

    impl PerfTuning {
        #[must_use]
        pub fn apply() -> Self {
            Self
        }
    }
}

pub use imp::PerfTuning;
