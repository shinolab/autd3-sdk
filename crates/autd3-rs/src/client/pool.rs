use std::sync::{Arc, Mutex, PoisonError};

use async_lock::{Semaphore, SemaphoreGuardArc};

use crate::protocol::{MAX_INFLIGHT, REPLY_DATA_BYTES_MAX};
use crate::response::Response;

#[derive(Clone, Copy)]
struct DeviceReplyData {
    len: u8,
    bytes: [u8; REPLY_DATA_BYTES_MAX],
}

impl DeviceReplyData {
    const EMPTY: Self = Self {
        len: 0,
        bytes: [0; REPLY_DATA_BYTES_MAX],
    };
}

pub(crate) struct ReplyBuffers {
    error_codes: Box<[u8]>,
    reply_data: Box<[DeviceReplyData]>,
}

impl ReplyBuffers {
    fn new(num_devices: usize) -> Self {
        Self {
            error_codes: vec![0u8; num_devices].into_boxed_slice(),
            reply_data: vec![DeviceReplyData::EMPTY; num_devices].into_boxed_slice(),
        }
    }

    pub(crate) fn reset(&mut self) {
        self.error_codes.fill(0);
        self.reply_data.fill(DeviceReplyData::EMPTY);
    }

    pub(crate) fn record_reply(&mut self, device: usize, error_code: u8, data: &[u8]) {
        self.error_codes[device] = error_code;
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let device_data = &mut self.reply_data[device];
        device_data.len = u8::try_from(len).expect("bounded by REPLY_DATA_BYTES_MAX");
        device_data.bytes[..len].copy_from_slice(&data[..len]);
    }

    pub(crate) fn response(&self, replied_devices: u128) -> Response {
        if self.reply_data.iter().all(|v| v.len == 0) {
            return Response::from_error_codes(&self.error_codes)
                .with_replied_devices(replied_devices);
        }
        Response::with_reply_data(
            &self.error_codes,
            self.reply_data
                .iter()
                .map(|v| v.bytes[..usize::from(v.len)].to_vec())
                .collect(),
        )
        .with_replied_devices(replied_devices)
    }
}

pub(crate) struct ReplySlot {
    pool: Arc<ReplySlotPool>,
    buffers: Option<ReplyBuffers>,
    _permit: SemaphoreGuardArc,
}

impl std::ops::Deref for ReplySlot {
    type Target = ReplyBuffers;

    fn deref(&self) -> &ReplyBuffers {
        self.buffers
            .as_ref()
            .expect("buffers are taken only on drop")
    }
}

impl std::ops::DerefMut for ReplySlot {
    fn deref_mut(&mut self) -> &mut ReplyBuffers {
        self.buffers
            .as_mut()
            .expect("buffers are taken only on drop")
    }
}

impl Drop for ReplySlot {
    fn drop(&mut self) {
        if let Some(buffers) = self.buffers.take() {
            self.pool.return_buffers(buffers);
        }
    }
}

pub(crate) struct ReplySlotPool {
    free_buffers: Mutex<Vec<ReplyBuffers>>,
    permits: Arc<Semaphore>,
}

impl ReplySlotPool {
    pub(crate) fn new(num_devices: usize) -> Arc<Self> {
        let free_buffers = (0..MAX_INFLIGHT)
            .map(|_| ReplyBuffers::new(num_devices))
            .collect();
        Arc::new(Self {
            free_buffers: Mutex::new(free_buffers),
            permits: Arc::new(Semaphore::new(MAX_INFLIGHT)),
        })
    }

    pub(crate) async fn acquire(self: &Arc<Self>) -> ReplySlot {
        let permit = self.permits.acquire_arc().await;
        let mut buffers = self
            .free_buffers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop()
            .expect("a permit guarantees a free slot");
        buffers.reset();
        ReplySlot {
            pool: Arc::clone(self),
            buffers: Some(buffers),
            _permit: permit,
        }
    }

    fn return_buffers(&self, buffers: ReplyBuffers) {
        self.free_buffers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(buffers);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn available(pool: &Arc<ReplySlotPool>) -> usize {
        let held: Vec<_> = std::iter::from_fn(|| pool.permits.try_acquire_arc()).collect();
        held.len()
    }

    fn acquire(pool: &Arc<ReplySlotPool>, n: usize) -> Vec<ReplySlot> {
        (0..n).map(|_| pollster::block_on(pool.acquire())).collect()
    }

    #[test]
    fn every_slot_up_to_the_device_queue_can_be_held() {
        let pool = ReplySlotPool::new(2);

        let held = acquire(&pool, MAX_INFLIGHT);

        assert_eq!(held.len(), MAX_INFLIGHT);
        assert_eq!(available(&pool), 0);
    }

    #[test]
    fn a_returned_slot_frees_its_permit() {
        let pool = ReplySlotPool::new(2);
        let mut held = acquire(&pool, MAX_INFLIGHT);

        held.pop();

        assert_eq!(available(&pool), 1);
        drop(held);
        assert_eq!(available(&pool), MAX_INFLIGHT);
    }
}
