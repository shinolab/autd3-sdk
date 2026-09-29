use crate::net::Mac;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RxMeta {
    pub port: u8,
}

pub trait Nic {
    fn send(&mut self, frame: &[u8], port: u8) -> bool;

    fn now(&mut self) -> Option<u64>;

    fn set_time(&mut self, ns: u64) -> bool;

    fn set_forwarding(&mut self, open: bool);

    fn set_mac(&mut self, mac: Mac);

    fn downstream_link(&mut self, port: u8) -> bool;

    fn arm_pulse(&mut self);

    fn stop_pulse(&mut self);

    fn pulse_ready(&mut self) -> bool;
}

#[must_use]
pub const fn other_port(port: u8) -> u8 {
    port ^ 1
}
