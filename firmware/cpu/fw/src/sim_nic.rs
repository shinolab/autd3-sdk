use std::vec::Vec;

use crate::net::Mac;
use crate::nic::Nic;

pub(crate) struct Sent {
    pub(crate) frame: Vec<u8>,
    pub(crate) port: u8,
}

pub(crate) struct SimNic {
    pub(crate) time: u64,
    pub(crate) time_set: Option<u64>,
    pub(crate) sent: Vec<Sent>,
    pub(crate) forwarding_open: Option<bool>,
    pub(crate) mac: Option<Mac>,
    pub(crate) link: bool,
    pub(crate) pulse_armed: u32,
    pub(crate) pulse_stopped: u32,
    pub(crate) pulse_ready: bool,
}

impl SimNic {
    pub(crate) fn new() -> Self {
        Self {
            time: 0,
            time_set: None,
            sent: Vec::new(),
            forwarding_open: None,
            mac: None,
            link: false,
            pulse_armed: 0,
            pulse_stopped: 0,
            pulse_ready: false,
        }
    }
}

impl Nic for SimNic {
    fn send(&mut self, frame: &[u8], port: u8) -> bool {
        self.sent.push(Sent {
            frame: frame.to_vec(),
            port,
        });
        true
    }

    fn now(&mut self) -> Option<u64> {
        Some(self.time)
    }

    fn set_time(&mut self, ns: u64) -> bool {
        self.time = ns;
        self.time_set = Some(ns);
        true
    }

    fn set_forwarding(&mut self, open: bool) {
        self.forwarding_open = Some(open);
    }

    fn set_mac(&mut self, mac: Mac) {
        self.mac = Some(mac);
    }

    fn downstream_link(&mut self, _port: u8) -> bool {
        self.link
    }

    fn arm_pulse(&mut self) {
        self.pulse_armed += 1;
    }

    fn stop_pulse(&mut self) {
        self.pulse_stopped += 1;
        self.pulse_ready = false;
    }

    fn pulse_ready(&mut self) -> bool {
        self.pulse_ready
    }
}
