use std::num::NonZeroU32;
use std::time::Duration;

use super::init::{INPUT_BYTES, OUTPUT_BYTES};
use crate::wire::{
    DATAGRAM_OVERHEAD_BYTES, ECAT_HEADER_BYTES, ETH_HEADER_BYTES, MIN_ETHERNET_FRAME_BYTES,
};

pub const DEFAULT_HOP_NS: u64 = 300;
pub const DEFAULT_LINK_SPEED_MBPS: u32 = 100;
pub const ETHERNET_FRAMING_BYTES: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireTiming {
    pub speed: NonZeroU32,
    pub hop: Duration,
}

impl Default for WireTiming {
    fn default() -> Self {
        Self {
            speed: NonZeroU32::new(DEFAULT_LINK_SPEED_MBPS).expect("the default speed is non-zero"),
            hop: Duration::from_nanos(DEFAULT_HOP_NS),
        }
    }
}

impl WireTiming {
    #[must_use]
    pub fn transmit(&self, frame_bytes: usize) -> Duration {
        let bits = u64::try_from(frame_bytes + ETHERNET_FRAMING_BYTES).unwrap_or(u64::MAX) * 8;
        Duration::from_nanos(bits * 1_000 / u64::from(self.speed.get()))
    }

    #[must_use]
    pub fn propagation(&self, devices: usize) -> Duration {
        self.hop * 2 * u32::try_from(devices).unwrap_or(u32::MAX)
    }
}

pub(crate) struct OutputChunk {
    pub(crate) offset: usize,
    pub(crate) len: usize,
    pub(crate) breaks_frame: bool,
}

pub(crate) const AL_STATUS_SPAN: usize = 6;

pub(crate) fn fixed_datagram_bytes(devices: usize) -> usize {
    (DATAGRAM_OVERHEAD_BYTES + 8)
        + (DATAGRAM_OVERHEAD_BYTES + 2)
        + (DATAGRAM_OVERHEAD_BYTES + AL_STATUS_SPAN)
        + (DATAGRAM_OVERHEAD_BYTES + 2)
        + (DATAGRAM_OVERHEAD_BYTES + devices * usize::from(INPUT_BYTES))
}

pub(crate) fn split_outputs(devices: usize, mtu: usize) -> (Vec<OutputChunk>, Vec<usize>) {
    let capacity = mtu - ECAT_HEADER_BYTES;
    let total = devices * usize::from(OUTPUT_BYTES);
    let mut chunks: Vec<OutputChunk> = Vec::new();
    let mut frame_bytes = Vec::new();
    let mut used = fixed_datagram_bytes(devices);
    let mut offset = 0usize;
    let mut breaks_frame = false;
    while offset < total {
        let available = capacity
            .saturating_sub(used)
            .saturating_sub(DATAGRAM_OVERHEAD_BYTES);
        if available == 0 {
            frame_bytes.push(used);
            used = 0;
            breaks_frame = true;
            continue;
        }
        let len = available.min(total - offset);
        chunks.push(OutputChunk {
            offset,
            len,
            breaks_frame,
        });
        breaks_frame = false;
        used += DATAGRAM_OVERHEAD_BYTES + len;
        offset += len;
    }
    frame_bytes.push(used);
    (chunks, frame_bytes)
}

#[must_use]
pub fn frame_wire_bytes(devices: usize, mtu: usize) -> Vec<usize> {
    let (_, frame_bytes) = split_outputs(devices, mtu);
    frame_bytes
        .into_iter()
        .map(|bytes| (bytes + ECAT_HEADER_BYTES + ETH_HEADER_BYTES).max(MIN_ETHERNET_FRAME_BYTES))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_faster_link_shortens_the_time_a_frame_takes() {
        let fast = WireTiming {
            speed: NonZeroU32::new(1_000).expect("non-zero"),
            ..WireTiming::default()
        };
        assert!(fast.transmit(1500) < WireTiming::default().transmit(1500));
        assert!(
            WireTiming::default().propagation(20) > WireTiming::default().propagation(10),
            "more devices means more hops",
        );
    }

    #[test]
    fn the_frame_count_grows_with_the_device_count() {
        let mut previous = 0;
        for devices in [1usize, 2, 4, 8, 10, 16, 20] {
            let bytes = frame_wire_bytes(devices, 1500);
            let total: usize = bytes.iter().sum();
            assert!(total > previous, "{devices} devices did not grow the wire");
            previous = total;
        }
    }

    #[test]
    fn every_frame_stays_within_the_mtu() {
        for devices in 1usize..=64 {
            for bytes in frame_wire_bytes(devices, 1500) {
                assert!(
                    bytes <= 1500 + ETH_HEADER_BYTES,
                    "{devices} devices: {bytes}"
                );
            }
        }
    }
}
