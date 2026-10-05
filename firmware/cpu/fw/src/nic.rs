use crate::net::Mac;

pub const NS_PER_SEC: u64 = 1_000_000_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TxStamp {
    pub ns: u32,
    pub overwritten: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RxMeta {
    pub port: u8,
    pub timestamp_ns: u32,
}

pub trait Nic {
    fn send(&mut self, frame: &[u8], port: u8, timestamp: bool) -> bool;

    fn now(&mut self) -> Option<u64>;

    fn step(&mut self, offset_ns: i64) -> bool;

    fn set_time(&mut self, ns: u64) -> bool;

    fn set_drift(&mut self, ppb: i32);

    fn clear_tx_timestamps(&mut self);

    fn take_tx_timestamp(&mut self, port: u8) -> Option<TxStamp>;

    fn set_forwarding(&mut self, open: bool);

    fn set_mac(&mut self, mac: Mac);

    fn downstream_link(&mut self, port: u8) -> bool;

    fn arm_pulse(&mut self);

    fn stop_pulse(&mut self);

    fn pulse_ready(&mut self) -> bool;
}

// This assumes a time of less than one second from recording to readout.
// In practice, it is called within a few ms such as inside a receive ISR or a 1 ms tick—this.
#[must_use]
pub fn complete(now_ns: u64, past_ns: u32) -> u64 {
    let sec = now_ns / NS_PER_SEC;
    let ns = now_ns % NS_PER_SEC;
    let sec = if u64::from(past_ns) <= ns {
        sec
    } else {
        sec.saturating_sub(1)
    };
    sec * NS_PER_SEC + u64::from(past_ns)
}

#[must_use]
pub const fn other_port(port: u8) -> u8 {
    port ^ 1
}
