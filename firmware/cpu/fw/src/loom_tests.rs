use std::vec::Vec;

use loom::sync::Arc;
use loom::sync::atomic::{AtomicU16, Ordering};
use loom::thread;

use crate::fifo::{FIFO_DEPTH, Fifo};

struct Ring {
    fifo: Fifo,
    slots: Vec<AtomicU16>,
}

impl Ring {
    fn new() -> Self {
        Self {
            fifo: Fifo::new(),
            slots: (0..FIFO_DEPTH).map(|_| AtomicU16::new(0)).collect(),
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
        self.fifo.begin_drain();
        let tail = self.fifo.next()?;
        let value = self.slots[Fifo::slot(tail)].load(Ordering::Relaxed);
        self.fifo.commit(tail);
        Some(value)
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
fn a_flush_requested_mid_drain_discards_only_the_queued_slots() {
    loom::model(|| {
        let ring = Arc::new(Ring::new());
        assert!(ring.push(1));
        assert!(ring.push(2));

        let producer = {
            let ring = Arc::clone(&ring);
            thread::spawn(move || {
                ring.fifo.request_flush(ring.fifo.head());
            })
        };

        let mut drained = Vec::new();
        while let Some(value) = ring.pop() {
            drained.push(value);
        }
        producer.join().unwrap();

        assert!(ring.pop().is_none());
        assert!(drained.iter().all(|&value| value == 1 || value == 2));
        assert!(drained.windows(2).all(|w| w[0] < w[1]));
    });
}
