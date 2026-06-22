use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::operation::Distribution;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

pub(super) struct Slot {
    num_devices: usize,
    dist: Distribution,
    payload: Box<[u8]>,
    cmds: Box<[Cmd]>,
    data: Box<[u8]>,
}

impl Slot {
    fn new(num_devices: usize) -> Self {
        Self {
            num_devices,
            dist: Distribution::Broadcast,
            payload: vec![0u8; num_devices * PAYLOAD_BYTES].into_boxed_slice(),
            cmds: vec![Cmd::Reset; num_devices].into_boxed_slice(),
            data: vec![0u8; num_devices].into_boxed_slice(),
        }
    }

    pub(super) fn reset(&mut self, dist: Distribution) {
        self.dist = dist;
        self.data.fill(0);
        let used = self.encode_devices_for(dist) * PAYLOAD_BYTES;
        self.payload[..used].fill(0);
    }

    fn encode_devices_for(&self, dist: Distribution) -> usize {
        match dist {
            Distribution::Broadcast => 1,
            Distribution::PerDevice => self.num_devices,
        }
    }

    pub(super) fn payload_mut(&mut self, device: usize) -> &mut [u8; PAYLOAD_BYTES] {
        let base = device * PAYLOAD_BYTES;
        (&mut self.payload[base..base + PAYLOAD_BYTES])
            .try_into()
            .expect("exact payload length")
    }

    pub(super) fn set_cmd(&mut self, device: usize, cmd: Cmd) {
        self.cmds[device] = cmd;
    }

    fn source(&self, device: usize) -> usize {
        match self.dist {
            Distribution::Broadcast => 0,
            Distribution::PerDevice => device,
        }
    }

    pub(super) fn cmd_for(&self, device: usize) -> Cmd {
        self.cmds[self.source(device)]
    }

    pub(super) fn payload_for(&self, device: usize) -> &[u8] {
        let base = self.source(device) * PAYLOAD_BYTES;
        &self.payload[base..base + PAYLOAD_BYTES]
    }

    pub(super) fn record_data(&mut self, device: usize, byte: u8) {
        self.data[device] = byte;
    }

    pub(super) fn data(&self) -> &[u8] {
        &self.data
    }
}

pub(super) struct SlotPool {
    free: Mutex<Vec<Slot>>,
    permits: Semaphore,
}

impl SlotPool {
    pub(super) fn new(num_devices: usize, capacity: usize) -> Arc<Self> {
        let free = (0..capacity).map(|_| Slot::new(num_devices)).collect();
        Arc::new(Self {
            free: Mutex::new(free),
            permits: Semaphore::new(capacity),
        })
    }

    pub(super) async fn acquire(&self) -> Slot {
        self.permits
            .acquire()
            .await
            .expect("pool semaphore is never closed")
            .forget();
        self.free
            .lock()
            .expect("pool mutex poisoned")
            .pop()
            .expect("a permit guarantees a free slot")
    }

    pub(super) fn release(&self, slot: Slot) {
        self.free.lock().expect("pool mutex poisoned").push(slot);
        self.permits.add_permits(1);
    }
}
