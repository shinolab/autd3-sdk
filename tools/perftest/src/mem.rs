pub struct MemProfile {
    pub allocations: u64,
    pub deallocations: u64,
    pub reallocations: u64,
    pub bytes_allocated: u64,
    pub bytes_deallocated: u64,
    pub sends: u64,
}

#[cfg(feature = "mem-profile")]
mod imp {
    use std::alloc::System;

    use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

    use super::MemProfile;

    #[global_allocator]
    static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

    pub struct Recorder(Region<'static, System>);

    pub fn start() -> Recorder {
        Recorder(Region::new(GLOBAL))
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn profile(recorder: Recorder, sends: u64) -> Option<MemProfile> {
        let Recorder(region) = recorder;
        let s = region.change();
        Some(MemProfile {
            allocations: u64::try_from(s.allocations).unwrap_or(0),
            deallocations: u64::try_from(s.deallocations).unwrap_or(0),
            reallocations: u64::try_from(s.reallocations).unwrap_or(0),
            bytes_allocated: u64::try_from(s.bytes_allocated).unwrap_or(0),
            bytes_deallocated: u64::try_from(s.bytes_deallocated).unwrap_or(0),
            sends,
        })
    }
}

#[cfg(not(feature = "mem-profile"))]
mod imp {
    use super::MemProfile;

    pub struct Recorder;

    pub fn start() -> Recorder {
        Recorder
    }

    pub fn profile(_recorder: Recorder, _sends: u64) -> Option<MemProfile> {
        None
    }
}

pub use imp::{profile, start};
