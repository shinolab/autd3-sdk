use std::time::{Duration, Instant};

use autd3_rs_core::protocol::{Cmd, FRAME_HEADER_BYTES, Seq, TxFrame};
use autd3_rs_core::{BusStats, DeviceClock, FRAME_BYTES_MAX};

use crate::udp::Reply;
pub(crate) use crate::udp::bus::BusTiming;

pub(crate) const DEFAULT_TIMING: BusTiming = BusTiming {
    heartbeat: Duration::from_millis(10),
    reply_timeout: Duration::from_millis(1),
};

#[derive(Clone)]
pub(crate) struct FrameBuf {
    bytes: [u8; FRAME_BYTES_MAX],
    len: usize,
}

impl FrameBuf {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: [0; FRAME_BYTES_MAX],
            len: FRAME_HEADER_BYTES,
        }
    }

    pub(crate) fn stage(&mut self, seq: Seq, cmd: Cmd, payload: &[u8]) {
        self.bytes[0] = seq.get();
        self.bytes[1] = cmd.as_u8();
        self.len = FRAME_HEADER_BYTES + payload.len();
        self.bytes[FRAME_HEADER_BYTES..self.len].copy_from_slice(payload);
    }

    pub(crate) fn stage_frame(&mut self, frame: &TxFrame) {
        self.len = frame.write_to(&mut self.bytes);
    }
}

impl AsRef<[u8]> for FrameBuf {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

pub(crate) trait Bus: Send + 'static {
    type Error: core::error::Error + Send + Sync + 'static;

    fn num_devices(&self) -> usize;

    fn stats(&self) -> BusStats {
        BusStats::default()
    }

    fn device_clock(&self) -> Option<DeviceClock> {
        None
    }

    fn timing(&self) -> BusTiming {
        DEFAULT_TIMING
    }

    fn next_msg_id(&self) -> u16;

    fn send(&mut self, frames: &[FrameBuf]) -> Result<u16, Self::Error>;

    fn heartbeat(&mut self) -> Result<u16, Self::Error>;

    fn try_recv(&mut self) -> Result<Option<Reply>, Self::Error>;

    fn wait_readable(&mut self, deadline: Instant) -> Result<bool, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
