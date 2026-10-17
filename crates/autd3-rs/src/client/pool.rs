use std::sync::{Arc, Mutex, PoisonError};

use async_lock::{Semaphore, SemaphoreGuardArc};

use crate::protocol::{MAX_INFLIGHT, REPLY_DATA_BYTES_MAX};
use crate::response::Response;

#[derive(Clone, Copy)]
struct ReplyValue {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

impl ReplyValue {
    const EMPTY: Self = Self {
        len: 0,
        bytes: [0; REPLY_DATA_BYTES_MAX],
    };
}

pub(crate) struct SlotData {
    status: Box<[u8]>,
    values: Box<[ReplyValue]>,
}

impl SlotData {
    fn new(num_devices: usize) -> Self {
        Self {
            status: vec![0u8; num_devices].into_boxed_slice(),
            values: vec![ReplyValue::EMPTY; num_devices].into_boxed_slice(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.status.fill(0);
        self.values.fill(ReplyValue::EMPTY);
    }

    pub(crate) fn record_reply(&mut self, device: usize, status: u8, data: &[u8]) {
        self.status[device] = status;
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let value = &mut self.values[device];
        value.len = u8::try_from(len).expect("bounded by REPLY_DATA_BYTES_MAX");
        value.bytes[..len].copy_from_slice(&data[..len]);
    }

    pub(crate) fn response(&self, replied: u128) -> Response {
        if self.values.iter().all(|v| v.len == 0) {
            return Response::from_status(&self.status).with_replied(replied);
        }
        Response::with_values(
            &self.status,
            self.values
                .iter()
                .map(|v| v.bytes[..usize::from(v.len)].to_vec())
                .collect(),
        )
        .with_replied(replied)
    }
}

pub(crate) struct Slot {
    pool: Arc<SlotPool>,
    data: Option<SlotData>,
    permit: Option<SemaphoreGuardArc>,
}

impl std::ops::Deref for Slot {
    type Target = SlotData;

    fn deref(&self) -> &SlotData {
        self.data.as_ref().expect("data is taken only on drop")
    }
}

impl std::ops::DerefMut for Slot {
    fn deref_mut(&mut self) -> &mut SlotData {
        self.data.as_mut().expect("data is taken only on drop")
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        if let Some(data) = self.data.take() {
            self.pool.release(data);
        }
        if let Some(permit) = self.permit.take() {
            self.pool.give_back(permit);
        }
    }
}

struct Limit {
    current: usize,
    debt: usize,
}

pub(crate) struct SlotPool {
    free: Mutex<Vec<SlotData>>,
    permits: Arc<Semaphore>,
    limit: Mutex<Limit>,
}

impl SlotPool {
    pub(crate) fn new(num_devices: usize, limit: usize) -> Arc<Self> {
        let free = (0..MAX_INFLIGHT.max(limit))
            .map(|_| SlotData::new(num_devices))
            .collect();
        Arc::new(Self {
            free: Mutex::new(free),
            permits: Arc::new(Semaphore::new(limit)),
            limit: Mutex::new(Limit {
                current: limit,
                debt: 0,
            }),
        })
    }

    pub(crate) fn set_limit(&self, limit: usize) {
        let mut state = self.limit.lock().unwrap_or_else(PoisonError::into_inner);
        if limit >= state.current {
            let grow = limit - state.current;
            let repaid = grow.min(state.debt);
            state.debt -= repaid;
            self.permits.add_permits(grow - repaid);
        } else {
            let mut shrink = state.current - limit;
            while shrink > 0 {
                let Some(permit) = self.permits.try_acquire_arc() else {
                    break;
                };
                permit.forget();
                shrink -= 1;
            }
            state.debt += shrink;
        }
        state.current = limit;
    }

    fn give_back(&self, permit: SemaphoreGuardArc) {
        let mut state = self.limit.lock().unwrap_or_else(PoisonError::into_inner);
        if state.debt > 0 {
            state.debt -= 1;
            permit.forget();
        } else {
            drop(state);
            drop(permit);
        }
    }

    pub(crate) async fn acquire(self: &Arc<Self>) -> Slot {
        let permit = self.permits.acquire_arc().await;
        let mut data = self
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop()
            .expect("a permit guarantees a free slot");
        data.reset();
        Slot {
            pool: Arc::clone(self),
            data: Some(data),
            permit: Some(permit),
        }
    }

    fn release(&self, data: SlotData) {
        self.free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(pool: &Arc<SlotPool>) -> usize {
        let held: Vec<_> = std::iter::from_fn(|| pool.permits.try_acquire_arc()).collect();
        held.len()
    }

    fn acquire(pool: &Arc<SlotPool>, n: usize) -> Vec<Slot> {
        (0..n).map(|_| pollster::block_on(pool.acquire())).collect()
    }

    #[test]
    fn raising_the_limit_frees_more_slots_at_once() {
        let pool = SlotPool::new(2, 2);
        let held = acquire(&pool, 2);
        assert_eq!(available(&pool), 0);

        pool.set_limit(MAX_INFLIGHT);

        assert_eq!(available(&pool), MAX_INFLIGHT - 2);
        drop(held);
        assert_eq!(available(&pool), MAX_INFLIGHT);
    }

    #[test]
    fn lowering_the_limit_takes_back_the_idle_slots_at_once() {
        let pool = SlotPool::new(2, MAX_INFLIGHT);

        pool.set_limit(2);

        assert_eq!(available(&pool), 2);
    }

    #[test]
    fn lowering_the_limit_below_the_slots_in_use_keeps_them_until_they_return() {
        let pool = SlotPool::new(2, 4);
        let mut held = acquire(&pool, 3);

        pool.set_limit(1);
        assert_eq!(available(&pool), 0);

        held.pop();
        assert_eq!(available(&pool), 0);
        held.pop();
        assert_eq!(available(&pool), 0);
        held.pop();
        assert_eq!(available(&pool), 1);
    }

    #[test]
    fn raising_the_limit_again_cancels_the_slots_still_owed() {
        let pool = SlotPool::new(2, 4);
        let held = acquire(&pool, 4);

        pool.set_limit(1);
        pool.set_limit(3);
        assert_eq!(available(&pool), 0);

        drop(held);
        assert_eq!(available(&pool), 3);
    }

    #[test]
    fn every_slot_up_to_the_device_queue_can_be_held_after_raising_the_limit() {
        let pool = SlotPool::new(2, 1);
        pool.set_limit(MAX_INFLIGHT);

        let held = acquire(&pool, MAX_INFLIGHT);

        assert_eq!(held.len(), MAX_INFLIGHT);
        assert_eq!(available(&pool), 0);
    }
}
