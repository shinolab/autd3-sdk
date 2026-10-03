use std::sync::{Arc, Mutex, PoisonError};

use autd3_rs_core::rt::Semaphore;

use crate::commands::operation::Distribution;
use crate::datagram::Datagram;
use crate::protocol::{Cmd, PAYLOAD_BYTES, REPLY_DATA_BYTES_MAX};
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

#[derive(Clone, Copy)]
struct FrameHead {
    cmd: Cmd,
    payload_len: usize,
}

impl FrameHead {
    const EMPTY: Self = Self {
        cmd: Cmd::Reset,
        payload_len: 0,
    };
}

pub(crate) struct SlotData {
    dist: Distribution,
    payload: Box<[u8]>,
    heads: Box<[FrameHead]>,
    status: Box<[u8]>,
    values: Box<[ReplyValue]>,
}

impl SlotData {
    fn new(num_devices: usize) -> Self {
        Self {
            dist: Distribution::Broadcast,
            payload: vec![0u8; num_devices * PAYLOAD_BYTES].into_boxed_slice(),
            heads: vec![FrameHead::EMPTY; num_devices].into_boxed_slice(),
            status: vec![0u8; num_devices].into_boxed_slice(),
            values: vec![ReplyValue::EMPTY; num_devices].into_boxed_slice(),
        }
    }

    pub(crate) fn reset(&mut self, dist: Distribution) {
        self.dist = dist;
        self.status.fill(0);
        self.values.fill(ReplyValue::EMPTY);
    }

    pub(crate) fn set(&mut self, device: usize, datagram: &Datagram) {
        let payload = datagram.payload();
        let base = device * PAYLOAD_BYTES;
        self.payload[base..base + payload.len()].copy_from_slice(payload);
        self.heads[device] = FrameHead {
            cmd: datagram.cmd,
            payload_len: payload.len(),
        };
    }

    fn source(&self, device: usize) -> usize {
        match self.dist {
            Distribution::Broadcast => 0,
            Distribution::PerDevice => device,
        }
    }

    pub(crate) fn cmd_for(&self, device: usize) -> Cmd {
        self.heads[self.source(device)].cmd
    }

    pub(crate) fn payload_for(&self, device: usize) -> &[u8] {
        let source = self.source(device);
        let base = source * PAYLOAD_BYTES;
        &self.payload[base..base + self.heads[source].payload_len]
    }

    pub(crate) fn record_reply(&mut self, device: usize, status: u8, data: &[u8]) {
        self.status[device] = status;
        let len = data.len().min(REPLY_DATA_BYTES_MAX);
        let value = &mut self.values[device];
        value.len = u8::try_from(len).expect("bounded by REPLY_DATA_BYTES_MAX");
        value.bytes[..len].copy_from_slice(&data[..len]);
    }

    pub(crate) fn response(&self) -> Response {
        if self.values.iter().all(|v| v.len == 0) {
            return Response::from_status(&self.status);
        }
        Response::with_values(
            &self.status,
            self.values
                .iter()
                .map(|v| v.bytes[..usize::from(v.len)].to_vec())
                .collect(),
        )
    }
}

pub(crate) struct Slot {
    pool: Arc<SlotPool>,
    data: Option<SlotData>,
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
    permits: Semaphore,
}

impl SlotPool {
    pub(crate) fn new(num_devices: usize, capacity: usize) -> Arc<Self> {
        let free = (0..capacity).map(|_| SlotData::new(num_devices)).collect();
        Arc::new(Self {
            free: Mutex::new(free),
            permits: Semaphore::new(capacity),
        })
    }

    pub(crate) async fn acquire(self: &Arc<Self>) -> Slot {
        self.permits.acquire().await.forget();
        let data = self
            .free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop()
            .expect("a permit guarantees a free slot");
        Slot {
            pool: Arc::clone(self),
            data: Some(data),
        }
    }

    #[cfg(test)]
    pub(crate) fn available_permits(&self) -> usize {
        self.permits.available_permits()
    }

    fn release(&self, data: SlotData) {
        self.free
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(data);
        self.permits.add_permits(1);
    }
}
