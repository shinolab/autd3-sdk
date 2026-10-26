use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub(crate) trait AtomicValue: Copy {
    type Atomic;
    const INIT: Self::Atomic;

    fn load(atomic: &Self::Atomic, order: Ordering) -> Self;
    fn store(atomic: &Self::Atomic, value: Self, order: Ordering);
}

impl AtomicValue for u32 {
    type Atomic = AtomicU32;
    const INIT: AtomicU32 = AtomicU32::new(0);

    fn load(atomic: &AtomicU32, order: Ordering) -> Self {
        atomic.load(order)
    }

    fn store(atomic: &AtomicU32, value: Self, order: Ordering) {
        atomic.store(value, order);
    }
}

pub(crate) struct OptionalAtomic<T: AtomicValue> {
    present: AtomicBool,
    value: T::Atomic,
}

impl<T: AtomicValue> OptionalAtomic<T> {
    pub(crate) const fn new() -> Self {
        Self {
            present: AtomicBool::new(false),
            value: T::INIT,
        }
    }

    pub(crate) fn store(&self, value: Option<T>) {
        match value {
            Some(value) => {
                T::store(&self.value, value, Ordering::Relaxed);
                self.present.store(true, Ordering::Release);
            }
            None => self.present.store(false, Ordering::Release),
        }
    }

    pub(crate) fn load(&self) -> Option<T> {
        self.present
            .load(Ordering::Acquire)
            .then(|| T::load(&self.value, Ordering::Relaxed))
    }
}
