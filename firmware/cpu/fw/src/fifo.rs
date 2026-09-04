use crate::sync::{AtomicU16, Ordering};

pub(crate) const FIFO_DEPTH: u16 = 8;
const FIFO_MASK: u16 = FIFO_DEPTH - 1;
const FIFO_CAPACITY: u16 = FIFO_DEPTH - 1;

pub(crate) struct Fifo {
    head: AtomicU16,
    tail: AtomicU16,
    flush_head: AtomicU16,
    flush_gen: AtomicU16,
    flush_seen: AtomicU16,
}

macro_rules! zeroed {
    () => {
        Self {
            head: AtomicU16::new(0),
            tail: AtomicU16::new(0),
            flush_head: AtomicU16::new(0),
            flush_gen: AtomicU16::new(0),
            flush_seen: AtomicU16::new(0),
        }
    };
}

#[cfg(not(loom))]
impl Fifo {
    pub(crate) const fn new() -> Self {
        zeroed!()
    }
}

#[cfg(loom)]
impl Fifo {
    pub(crate) fn new() -> Self {
        zeroed!()
    }
}

impl Fifo {
    pub(crate) fn slot(index: u16) -> usize {
        (index & FIFO_MASK) as usize
    }

    pub(crate) fn is_full(head: u16, tail: u16) -> bool {
        head.wrapping_sub(tail) >= FIFO_CAPACITY
    }

    pub(crate) fn reset(&self) {
        self.head.store(0, Ordering::Relaxed);
        self.tail.store(0, Ordering::Relaxed);
        self.flush_head.store(0, Ordering::Relaxed);
        self.flush_gen.store(0, Ordering::Relaxed);
        self.flush_seen.store(0, Ordering::Relaxed);
    }

    pub(crate) fn head(&self) -> u16 {
        self.head.load(Ordering::Relaxed)
    }

    pub(crate) fn tail_acquire(&self) -> u16 {
        self.tail.load(Ordering::Acquire)
    }

    pub(crate) fn request_flush(&self, head: u16) {
        self.flush_head.store(head, Ordering::Relaxed);
        self.flush_gen.store(
            self.flush_gen.load(Ordering::Relaxed).wrapping_add(1),
            Ordering::Release,
        );
    }

    pub(crate) fn publish(&self, head: u16) {
        self.head.store(head.wrapping_add(1), Ordering::Release);
    }

    pub(crate) fn begin_drain(&self) -> u16 {
        let generation = self.flush_gen.load(Ordering::Acquire);
        if generation != self.flush_seen.load(Ordering::Relaxed) {
            self.flush_seen.store(generation, Ordering::Relaxed);
            let flush_head = self.flush_head.load(Ordering::Relaxed);
            if flush_head.wrapping_sub(self.tail.load(Ordering::Relaxed)) < FIFO_DEPTH {
                self.tail.store(flush_head, Ordering::Release);
            }
        }
        generation
    }

    pub(crate) fn next(&self) -> Option<u16> {
        let tail = self.tail.load(Ordering::Relaxed);
        if tail == self.head.load(Ordering::Acquire) {
            None
        } else {
            Some(tail)
        }
    }

    pub(crate) fn commit(&self, tail: u16) {
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
    }

    pub(crate) fn flush_gen(&self) -> u16 {
        self.flush_gen.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn seed(&self, head: u16, tail: u16) {
        self.head.store(head, Ordering::Relaxed);
        self.tail.store(tail, Ordering::Relaxed);
    }
}
