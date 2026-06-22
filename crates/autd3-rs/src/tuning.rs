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
        priority_set: bool,
    }

    impl PerfTuning {
        #[must_use]
        pub fn apply() -> Self {
            // SAFETY: timeBeginPeriod is a thread-safe winmm call; it is paired
            // with timeEndPeriod(TIMER_PERIOD_MS) in Drop.
            let timer_set = unsafe { timeBeginPeriod(TIMER_PERIOD_MS) } == TIMERR_NOERROR;
            // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no
            // close; SetPriorityClass only reads it.
            let priority_set =
                unsafe { SetPriorityClass(GetCurrentProcess(), HIGH_PRIORITY_CLASS) != 0 };
            Self {
                timer_set,
                priority_set,
            }
        }

        #[must_use]
        pub fn timer_boosted(&self) -> bool {
            self.timer_set
        }

        #[must_use]
        pub fn high_priority(&self) -> bool {
            self.priority_set
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

        #[must_use]
        pub fn timer_boosted(&self) -> bool {
            false
        }

        #[must_use]
        pub fn high_priority(&self) -> bool {
            false
        }
    }
}

pub use imp::PerfTuning;
