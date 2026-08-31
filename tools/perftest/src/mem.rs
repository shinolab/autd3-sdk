pub struct SizeBucket {
    pub size: usize,
    pub count: u64,
}

pub struct MemProfile {
    pub allocations: u64,
    pub deallocations: u64,
    pub reallocations: u64,
    pub bytes_allocated: u64,
    pub bytes_deallocated: u64,
    pub sends: u64,
    pub top_sizes: Vec<SizeBucket>,
    pub large_count: u64,
    pub large_bytes: u64,
}

#[cfg(feature = "mem-profile")]
mod imp {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use super::{MemProfile, SizeBucket};

    const HIST_LIMIT: usize = 8192;
    const TOP_SIZES: usize = 10;

    static RECORDING: AtomicBool = AtomicBool::new(false);
    static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
    static BYTES_ALLOCATED: AtomicU64 = AtomicU64::new(0);
    static BYTES_DEALLOCATED: AtomicU64 = AtomicU64::new(0);
    static LARGE_COUNT: AtomicU64 = AtomicU64::new(0);
    static LARGE_BYTES: AtomicU64 = AtomicU64::new(0);
    static HIST: [AtomicU64; HIST_LIMIT] = [const { AtomicU64::new(0) }; HIST_LIMIT];

    #[global_allocator]
    static GLOBAL: Histogram = Histogram;

    struct Histogram;

    fn record(size: usize) {
        if size < HIST_LIMIT {
            HIST[size].fetch_add(1, Ordering::Relaxed);
        } else {
            LARGE_COUNT.fetch_add(1, Ordering::Relaxed);
            LARGE_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
    }

    unsafe impl GlobalAlloc for Histogram {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if RECORDING.load(Ordering::Relaxed) {
                ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                BYTES_ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
                record(layout.size());
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            if RECORDING.load(Ordering::Relaxed) {
                DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                BYTES_DEALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if RECORDING.load(Ordering::Relaxed) {
                REALLOCATIONS.fetch_add(1, Ordering::Relaxed);
                BYTES_ALLOCATED.fetch_add(
                    new_size.saturating_sub(layout.size()) as u64,
                    Ordering::Relaxed,
                );
                record(new_size);
            }
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    pub struct Recorder;

    pub fn start() -> Recorder {
        RECORDING.store(true, Ordering::SeqCst);
        Recorder
    }

    #[allow(clippy::unnecessary_wraps)]
    pub fn profile(_recorder: Recorder, sends: u64) -> Option<MemProfile> {
        RECORDING.store(false, Ordering::SeqCst);

        let mut top_sizes: Vec<SizeBucket> = HIST
            .iter()
            .enumerate()
            .filter_map(|(size, count)| {
                let count = count.load(Ordering::Relaxed);
                (count > 0).then_some(SizeBucket { size, count })
            })
            .collect();
        top_sizes.sort_unstable_by_key(|b| std::cmp::Reverse(b.size as u64 * b.count));
        top_sizes.truncate(TOP_SIZES);

        Some(MemProfile {
            allocations: ALLOCATIONS.load(Ordering::Relaxed),
            deallocations: DEALLOCATIONS.load(Ordering::Relaxed),
            reallocations: REALLOCATIONS.load(Ordering::Relaxed),
            bytes_allocated: BYTES_ALLOCATED.load(Ordering::Relaxed),
            bytes_deallocated: BYTES_DEALLOCATED.load(Ordering::Relaxed),
            sends,
            top_sizes,
            large_count: LARGE_COUNT.load(Ordering::Relaxed),
            large_bytes: LARGE_BYTES.load(Ordering::Relaxed),
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
