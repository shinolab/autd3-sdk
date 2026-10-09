use crate::sync::{AtomicU16, Ordering};

pub(crate) const FIFO_DEPTH: u16 = 8;
const FIFO_MASK: u16 = FIFO_DEPTH - 1;
const FIFO_CAPACITY: u16 = FIFO_DEPTH - 1;

const _: () = assert!(FIFO_DEPTH.is_power_of_two());

pub(crate) struct Fifo {
    head: AtomicU16,
    tail: AtomicU16,
}

impl Fifo {
    #[const_fn::const_fn(cfg(not(loom)))]
    pub(crate) const fn new() -> Self {
        Self {
            head: AtomicU16::new(0),
            tail: AtomicU16::new(0),
        }
    }
}

impl Fifo {
    pub(crate) fn slot(index: u16) -> usize {
        (index & FIFO_MASK) as usize
    }

    pub(crate) fn is_full(head: u16, tail: u16) -> bool {
        head.wrapping_sub(tail) >= FIFO_CAPACITY
    }

    pub(crate) fn head(&self) -> u16 {
        self.head.load(Ordering::Relaxed)
    }

    pub(crate) fn tail_acquire(&self) -> u16 {
        self.tail.load(Ordering::Acquire)
    }

    pub(crate) fn publish(&self, head: u16) {
        self.head.store(head.wrapping_add(1), Ordering::Release);
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

    #[cfg(test)]
    pub(crate) fn seed(&self, head: u16, tail: u16) {
        self.head.store(head, Ordering::Relaxed);
        self.tail.store(tail, Ordering::Relaxed);
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use super::{FIFO_DEPTH, Fifo};

    #[test]
    fn fifo_indices_wrap_without_confusing_full_and_empty() {
        let fifo = Fifo::new();
        let start = u16::MAX - 2;
        fifo.seed(start, start);
        assert!(fifo.next().is_none());

        let mut head = start;
        let mut pushed = 0;
        while !Fifo::is_full(head, fifo.tail_acquire()) {
            fifo.publish(head);
            head = head.wrapping_add(1);
            pushed += 1;
        }
        assert_eq!(pushed, FIFO_DEPTH - 1);
        assert!(Fifo::is_full(fifo.head(), fifo.tail_acquire()));

        let mut drained = 0;
        while let Some(tail) = fifo.next() {
            fifo.commit(tail);
            drained += 1;
        }
        assert_eq!(drained, pushed);
        assert!(!Fifo::is_full(fifo.head(), fifo.tail_acquire()));
    }

    #[test]
    fn fifo_slots_stay_distinct_across_the_index_wrap() {
        let start = u16::MAX - 3;
        let mut seen = [false; FIFO_DEPTH as usize];
        for i in 0..FIFO_DEPTH {
            let slot = Fifo::slot(start.wrapping_add(i));
            assert!(!seen[slot]);
            seen[slot] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use std::vec::Vec;

    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicU16, Ordering};
    use loom::thread;

    use super::{FIFO_DEPTH, Fifo};

    struct Ring {
        fifo: Fifo,
        slots: Vec<AtomicU16>,
        results: Vec<AtomicU16>,
    }

    impl Ring {
        fn new() -> Self {
            Self {
                fifo: Fifo::new(),
                slots: (0..FIFO_DEPTH).map(|_| AtomicU16::new(0)).collect(),
                results: (0..FIFO_DEPTH).map(|_| AtomicU16::new(0)).collect(),
            }
        }

        fn seeded(head: u16, tail: u16) -> Self {
            let ring = Self::new();
            ring.fifo.seed(head, tail);
            ring
        }

        fn push(&self, value: u16) -> bool {
            let head = self.fifo.head();
            let tail = self.fifo.tail_acquire();
            if Fifo::is_full(head, tail) {
                return false;
            }
            self.slots[Fifo::slot(head)].store(value, Ordering::Relaxed);
            self.fifo.publish(head);
            true
        }

        fn pop(&self) -> Option<u16> {
            let tail = self.fifo.next()?;
            let value = self.slots[Fifo::slot(tail)].load(Ordering::Relaxed);
            self.results[Fifo::slot(tail)].store(value, Ordering::Relaxed);
            self.fifo.commit(tail);
            Some(value)
        }

        fn reply(&self) -> (u16, u16) {
            let tail = self.fifo.tail_acquire();
            let result = self.results[Fifo::slot(tail.wrapping_sub(1))].load(Ordering::Relaxed);
            (tail, result)
        }

        fn drain_until(&self, wanted: usize) -> Vec<u16> {
            let mut got = Vec::with_capacity(wanted);
            while got.len() < wanted {
                match self.pop() {
                    Some(value) => got.push(value),
                    None => thread::yield_now(),
                }
            }
            got
        }
    }

    #[test]
    fn published_slots_are_never_read_before_they_are_written() {
        loom::model(|| {
            let ring = Arc::new(Ring::new());
            let producer = {
                let ring = Arc::clone(&ring);
                thread::spawn(move || {
                    for value in 1..=2u16 {
                        assert!(ring.push(value));
                    }
                })
            };
            let got = ring.drain_until(2);
            producer.join().unwrap();
            assert_eq!(got, [1, 2]);
        });
    }

    #[test]
    fn a_full_ring_never_overwrites_an_unconsumed_slot() {
        const CAPACITY: u16 = FIFO_DEPTH - 1;

        loom::model(|| {
            let ring = Arc::new(Ring::seeded(CAPACITY, 0));
            for index in 0..CAPACITY {
                ring.slots[Fifo::slot(index)].store(index + 1, Ordering::Relaxed);
            }

            let producer = {
                let ring = Arc::clone(&ring);
                thread::spawn(move || {
                    let mut accepted = Vec::new();
                    for value in [u16::MAX - 1, u16::MAX] {
                        if ring.push(value) {
                            accepted.push(value);
                        }
                    }
                    accepted
                })
            };
            let first = ring.pop();
            let accepted = producer.join().unwrap();

            let mut drained = Vec::new();
            drained.extend(first);
            while let Some(value) = ring.pop() {
                drained.push(value);
            }

            let mut expected: Vec<u16> = (1..=CAPACITY).collect();
            expected.extend(accepted);
            assert_eq!(drained, expected);
        });
    }

    #[test]
    fn the_published_reply_is_the_result_of_the_last_committed_slot() {
        loom::model(|| {
            let ring = Arc::new(Ring::new());
            let producer = {
                let ring = Arc::clone(&ring);
                thread::spawn(move || {
                    for value in 1..=2u16 {
                        assert!(ring.push(value));
                        let (tail, result) = ring.reply();
                        assert_eq!(result, tail);
                    }
                })
            };
            let got = ring.drain_until(2);
            producer.join().unwrap();
            assert_eq!(got, [1, 2]);
            assert_eq!(ring.reply(), (2, 2));
        });
    }
}
