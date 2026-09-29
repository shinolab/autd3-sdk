use autd3_cpu_fw::proto::{Disposition, Drained, Reply};

use crate::device::Device;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fault {
    pub drop_frames: usize,
    pub drop_replies: usize,
    pub device: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditReply {
    pub device: usize,
    pub msg_id: u16,
    pub reply: Reply,
}

pub struct Audit {
    devices: Vec<Device>,
    fault: Fault,
}

impl Audit {
    #[must_use]
    pub fn new(num_transducers: impl IntoIterator<Item = usize>) -> Self {
        Self {
            devices: num_transducers.into_iter().map(Device::new).collect(),
            fault: Fault::default(),
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

    pub fn inject(&mut self, fault: Fault) {
        self.fault = fault;
    }

    #[must_use]
    pub fn pending_fault(&self) -> Fault {
        self.fault
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.devices.len()
    }

    fn targeted(&self, device: usize) -> bool {
        self.fault.device.is_none_or(|d| d == device)
    }

    fn deliver(&mut self, replies: &mut Vec<AuditReply>, device: usize, msg_id: u16) {
        if self.targeted(device) && self.fault.drop_replies > 0 {
            return;
        }
        replies.push(AuditReply {
            device,
            msg_id,
            reply: self.devices[device].reply(),
        });
    }

    pub fn send(&mut self, frames: &[&[u8]], msg_id: u16) -> Vec<AuditReply> {
        let dropping = self.fault.drop_frames > 0;
        let losing = self.fault.drop_replies > 0;
        let mut replies = Vec::new();
        for (device, frame) in frames.iter().enumerate().take(self.devices.len()) {
            if dropping && self.targeted(device) {
                continue;
            }
            match self.devices[device].recv(frame, msg_id) {
                Disposition::Reply => self.deliver(&mut replies, device, msg_id),
                Disposition::Deferred => loop {
                    match self.devices[device].process_one() {
                        Drained::Empty => break,
                        Drained::Completed { msg_id } => {
                            self.deliver(&mut replies, device, msg_id);
                        }
                        Drained::Flushed => {}
                    }
                },
                Disposition::Dropped => {}
            }
        }
        if dropping {
            self.fault.drop_frames -= 1;
        }
        if losing {
            self.fault.drop_replies -= 1;
        }
        replies
    }

    pub fn heartbeat(&mut self, msg_id: u16) -> Vec<AuditReply> {
        let losing = self.fault.drop_replies > 0;
        let mut replies = Vec::new();
        for device in 0..self.devices.len() {
            self.deliver(&mut replies, device, msg_id);
        }
        if losing {
            self.fault.drop_replies -= 1;
        }
        replies
    }
}
