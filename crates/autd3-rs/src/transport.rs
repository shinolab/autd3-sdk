use std::time::{Duration, Instant};

use autd3_rs_core::{BusStats, DeviceClock, FRAME_BYTES_MAX};

use crate::udp::Reply;
pub(crate) use crate::udp::bus::BusTiming;

pub(crate) const DEFAULT_TIMING: BusTiming = BusTiming {
    heartbeat: Duration::from_millis(10),
    reply_timeout: Duration::from_millis(1),
};

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

    fn send(&mut self, frames: &[[u8; FRAME_BYTES_MAX]]) -> Result<u16, Self::Error>;

    fn heartbeat(&mut self) -> Result<u16, Self::Error>;

    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
