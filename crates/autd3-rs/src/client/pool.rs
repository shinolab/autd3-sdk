use std::sync::{Arc, Mutex, PoisonError};

use async_lock::{Semaphore, SemaphoreGuardArc};

use crate::protocol::REPLY_DATA_BYTES_MAX;
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
    _permit: SemaphoreGuardArc,
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
    }
}

pub(crate) struct SlotPool {
    free: Mutex<Vec<SlotData>>,
    permits: Arc<Semaphore>,
}

impl SlotPool {
    pub(crate) fn new(num_devices: usize, capacity: usize) -> Arc<Self> {
        let free = (0..capacity).map(|_| SlotData::new(num_devices)).collect();
        Arc::new(Self {
            free: Mutex::new(free),
            permits: Arc::new(Semaphore::new(capacity)),
        })
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
            _permit: permit,
        }
    }

    fn release(&self, data: SlotData) {
        self.free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(data);
    }
}
