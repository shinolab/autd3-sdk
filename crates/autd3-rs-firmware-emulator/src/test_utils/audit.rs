use autd3_cpu_fw::proto::{Disposition, Drained, Reply};

use crate::device::Device;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditReply {
    pub device: usize,
    pub msg_id: u16,
    pub reply: Reply,
}

pub struct Audit {
    devices: Vec<Device>,
}

impl Audit {
    #[must_use]
    pub fn new(num_transducers: impl IntoIterator<Item = usize>) -> Self {
        Self {
            devices: num_transducers.into_iter().map(Device::new).collect(),
        }
    }

    #[must_use]
    pub fn device(&self, idx: usize) -> &Device {
        &self.devices[idx]
    }

    #[must_use]
    pub fn device_mut(&mut self, idx: usize) -> &mut Device {
        &mut self.devices[idx]
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.devices.len()
    }

    fn deliver(&self, replies: &mut Vec<AuditReply>, device: usize, msg_id: u16) {
        replies.push(AuditReply {
            device,
            msg_id,
            reply: self.devices[device].reply(),
        });
    }

    pub fn send(&mut self, frames: &[&[u8]], msg_id: u16) -> Vec<AuditReply> {
        let mut replies = Vec::new();
        for (device, frame) in frames.iter().enumerate().take(self.devices.len()) {
            if self.devices[device].recv(frame, msg_id) == Disposition::Dropped {
                continue;
            }
            while let Drained::Completed { msg_id } = self.devices[device].process_one() {
                self.deliver(&mut replies, device, msg_id);
            }
        }
        replies
    }
}
