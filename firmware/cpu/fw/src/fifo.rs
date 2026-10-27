use core::ops::Deref;

use crate::sync::{AtomicU16, Ordering};

pub(crate) const FIFO_DEPTH: u16 = 8;
const FIFO_MASK: u16 = FIFO_DEPTH - 1;
const FIFO_CAPACITY: u16 = FIFO_DEPTH - 1;

const _: () = assert!(FIFO_DEPTH.is_power_of_two());

pub(crate) struct Fifo<S> {
    head: AtomicU16,
    tail: AtomicU16,
    slots: [S; FIFO_DEPTH as usize],
}

impl<S> Fifo<S> {
    #[const_fn::const_fn(cfg(not(loom)))]
    pub(crate) const fn new(slots: [S; FIFO_DEPTH as usize]) -> Self {
        Self {
            head: AtomicU16::new(0),
            tail: AtomicU16::new(0),
            slots,
        }
    }

    fn slot(&self, index: u16) -> &S {
        &self.slots[(index & FIFO_MASK) as usize]
    }

    pub(crate) fn push(&self, write: impl FnOnce(&S) -> bool) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= FIFO_CAPACITY || !write(self.slot(head)) {
            return false;
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    pub(crate) fn front(&self) -> Option<Front<'_, S>> {
        let tail = self.tail.load(Ordering::Relaxed);
        (tail != self.head.load(Ordering::Acquire)).then_some(Front { fifo: self, tail })
    }

    pub(crate) fn last_popped(&self) -> &S {
        self.slot(self.tail.load(Ordering::Acquire).wrapping_sub(1))
    }

    #[cfg(test)]
    pub(crate) fn slots(&self) -> &[S] {
        &self.slots
    }

    #[cfg(test)]
    fn seed(&self, head: u16, tail: u16) {
        self.head.store(head, Ordering::Relaxed);
        self.tail.store(tail, Ordering::Relaxed);
    }
}

pub(crate) struct Front<'a, S> {
    fifo: &'a Fifo<S>,
    tail: u16,
}

impl<S> Front<'_, S> {
    pub(crate) fn pop(self) {
        self.fifo
            .tail
            .store(self.tail.wrapping_add(1), Ordering::Release);
    }
}

impl<S> Deref for Front<'_, S> {
    type Target = S;

    fn deref(&self) -> &S {
        self.fifo.slot(self.tail)
    }
}

#[cfg(all(test, not(loom)))]
mod tests {
    use core::cell::Cell;

    use super::{FIFO_CAPACITY, FIFO_DEPTH, Fifo};

    fn fifo() -> Fifo<Cell<u16>> {
        Fifo::new([const { Cell::new(0) }; FIFO_DEPTH as usize])
    }

    fn push(fifo: &Fifo<Cell<u16>>, value: u16) -> bool {
        fifo.push(|slot| {
            slot.set(value);
            true
        })
    }

    fn pop(fifo: &Fifo<Cell<u16>>) -> Option<u16> {
        let front = fifo.front()?;
        let value = front.get();
        front.pop();
        Some(value)
    }

    #[test]
    fn fifo_pops_in_push_order_across_the_index_wrap_without_confusing_full_and_empty() {
        let fifo = fifo();
        let start = u16::MAX - 2;
        fifo.seed(start, start);
        assert!(fifo.front().is_none());

        for value in 1..=FIFO_CAPACITY {
            assert!(push(&fifo, value));
        }
        assert!(!push(&fifo, u16::MAX));

        let drained: std::vec::Vec<u16> = core::iter::from_fn(|| pop(&fifo)).collect();
        assert_eq!(drained, (1..=FIFO_CAPACITY).collect::<std::vec::Vec<_>>());
        assert_eq!(fifo.last_popped().get(), FIFO_CAPACITY);
        assert!(push(&fifo, u16::MAX));
    }

    #[test]
    fn a_refused_write_does_not_publish_the_slot() {
        let fifo = fifo();
        assert!(!fifo.push(|_| false));
        assert!(fifo.front().is_none());
    }
}

#[cfg(all(test, loom))]
mod loom_tests {
    use std::vec::Vec;

    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicU16, Ordering};
    use loom::thread;

    use super::{FIFO_CAPACITY, Fifo};

    struct TestSlot {
        value: AtomicU16,
        result: AtomicU16,
    }

    struct Ring {
        fifo: Fifo<TestSlot>,
    }

    impl Ring {
        fn new() -> Self {
            Self {
                fifo: Fifo::new(core::array::from_fn(|_| TestSlot {
                    value: AtomicU16::new(0),
                    result: AtomicU16::new(0),
                })),
            }
        }

        fn seeded(head: u16, tail: u16) -> Self {
            let ring = Self::new();
            ring.fifo.seed(head, tail);
            ring
        }

        fn push(&self, value: u16) -> bool {
            self.fifo.push(|slot| {
                slot.value.store(value, Ordering::Relaxed);
                true
            })
        }

        fn pop(&self) -> Option<u16> {
            let front = self.fifo.front()?;
            let value = front.value.load(Ordering::Relaxed);
            front.result.store(value, Ordering::Relaxed);
            front.pop();
            Some(value)
        }

        fn reply(&self) -> (u16, u16) {
            let slot = self.fifo.last_popped();
            (
                slot.value.load(Ordering::Relaxed),
                slot.result.load(Ordering::Relaxed),
            )
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
        loom::model(|| {
            let ring = Arc::new(Ring::seeded(FIFO_CAPACITY, 0));
            for (index, slot) in ring
                .fifo
                .slots()
                .iter()
                .take(FIFO_CAPACITY.into())
                .enumerate()
            {
                slot.value.store(index as u16 + 1, Ordering::Relaxed);
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

            let mut expected: Vec<u16> = (1..=FIFO_CAPACITY).collect();
            expected.extend(accepted);
            assert_eq!(drained, expected);
        });
    }

    #[test]
    fn the_last_popped_slot_carries_its_result() {
        loom::model(|| {
            let ring = Arc::new(Ring::new());
            let producer = {
                let ring = Arc::clone(&ring);
                thread::spawn(move || {
                    for value in 1..=2u16 {
                        assert!(ring.push(value));
                        let (value, result) = ring.reply();
                        assert_eq!(result, value);
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
