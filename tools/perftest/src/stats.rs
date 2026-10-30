use std::collections::BTreeMap;
use std::time::Duration;

use autd3_cpu_wire::udp::{FrameReply, Header};
use autd3_rs::Frame;
use autd3_rs::protocol::FrameHeader;

const LINK_BITS_PER_SEC: f64 = 100_000_000.0;
const WIRE_OVERHEAD_BYTES: u64 = 8 + 40 + 14 + 4 + 8 + 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampleStatus {
    Ok,
    DeviceError(u8),
    Timeout,
    NetworkError,
}

#[derive(Default)]
pub(crate) struct Counters {
    pub ok: u64,
    pub timeouts: u64,
    pub device_errors: BTreeMap<u8, u64>,
    pub network_errors: u64,
}

impl Counters {
    pub(crate) fn observe(&mut self, status: SampleStatus) {
        match status {
            SampleStatus::Ok => self.ok += 1,
            SampleStatus::Timeout => self.timeouts += 1,
            SampleStatus::NetworkError => self.network_errors += 1,
            SampleStatus::DeviceError(code) => *self.device_errors.entry(code).or_insert(0) += 1,
        }
    }

    pub(crate) fn device_error_total(&self) -> u64 {
        self.device_errors.values().sum()
    }

    pub(crate) fn total(&self) -> u64 {
        self.ok + self.timeouts + self.network_errors + self.device_error_total()
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Traffic {
    devices: usize,
    tx_frames: u64,
    tx_datagrams: u64,
    tx_bytes: u64,
    rx_datagrams: u64,
    rx_bytes: u64,
}

impl Traffic {
    pub(crate) const fn new(devices: usize) -> Self {
        Self {
            devices,
            tx_frames: 0,
            tx_datagrams: 0,
            tx_bytes: 0,
            rx_datagrams: 0,
            rx_bytes: 0,
        }
    }

    pub(crate) const fn frames(&self) -> u64 {
        self.tx_frames
    }

    pub(crate) fn sent(&mut self, frame: &Frame<'_>) {
        self.tx_frames += 1;
        for device in 0..self.devices {
            self.tx_datagrams += 1;
            self.tx_bytes += (size_of::<Header>()
                + size_of::<FrameHeader>()
                + frame.datagram_for(device).payload_len) as u64;
        }
    }

    pub(crate) fn replied(&mut self) {
        self.rx_datagrams += self.devices as u64;
        self.rx_bytes += (self.devices * (size_of::<Header>() + size_of::<FrameReply>())) as u64;
    }

    pub(crate) const fn since(self, before: Self) -> Self {
        Self {
            devices: self.devices,
            tx_frames: self.tx_frames.saturating_sub(before.tx_frames),
            tx_datagrams: self.tx_datagrams.saturating_sub(before.tx_datagrams),
            tx_bytes: self.tx_bytes.saturating_sub(before.tx_bytes),
            rx_datagrams: self.rx_datagrams.saturating_sub(before.rx_datagrams),
            rx_bytes: self.rx_bytes.saturating_sub(before.rx_bytes),
        }
    }

    pub(crate) fn rate(self, elapsed: Duration) -> TrafficRate {
        let secs = elapsed.as_secs_f64();
        if secs <= 0.0 {
            return TrafficRate::default();
        }
        let line_percent = |datagrams: u64, bytes: u64| {
            (bytes + datagrams * WIRE_OVERHEAD_BYTES) as f64 * 8.0 / secs / LINK_BITS_PER_SEC
                * 100.0
        };
        TrafficRate {
            tx_byte_per_sec: self.tx_bytes as f64 / secs,
            line_down_percent: line_percent(self.tx_datagrams, self.tx_bytes),
            line_up_percent: line_percent(self.rx_datagrams, self.rx_bytes),
        }
    }
}

#[derive(Default)]
pub(crate) struct TrafficRate {
    pub tx_byte_per_sec: f64,
    pub line_down_percent: f64,
    pub line_up_percent: f64,
}

#[derive(Default)]
pub(crate) struct LatencyStats {
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
}

pub(crate) fn rtt_histogram() -> hdrhistogram::Histogram<u64> {
    hdrhistogram::Histogram::new(3).expect("3 significant digits is a valid precision")
}

pub(crate) fn record_rtt(rtts: &mut hdrhistogram::Histogram<u64>, rtt: Duration) {
    rtts.saturating_record(u64::try_from(rtt.as_nanos()).unwrap_or(u64::MAX));
}

pub(crate) fn latency(rtts: &hdrhistogram::Histogram<u64>) -> LatencyStats {
    if rtts.is_empty() {
        return LatencyStats::default();
    }
    let percentile = |quantile: f64| Duration::from_nanos(rtts.value_at_quantile(quantile));
    LatencyStats {
        mean: Duration::from_secs_f64(rtts.mean() / 1e9),
        p50: percentile(0.50),
        p95: percentile(0.95),
        p99: percentile(0.99),
        min: Duration::from_nanos(rtts.min()),
        max: Duration::from_nanos(rtts.max()),
    }
}

#[derive(Default)]
pub(crate) struct AckLatency {
    frames: u64,
    total: Duration,
    max: Duration,
}

impl AckLatency {
    pub(crate) fn record(&mut self, latency: Duration) {
        self.frames += 1;
        self.total += latency;
        self.max = self.max.max(latency);
    }

    pub(crate) const fn frames(&self) -> u64 {
        self.frames
    }

    pub(crate) fn mean(&self) -> Option<Duration> {
        (self.frames > 0).then(|| {
            let nanos = self.total.as_nanos() / u128::from(self.frames);
            Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
        })
    }

    pub(crate) const fn max(&self) -> Duration {
        self.max
    }
}

pub(crate) struct Summary {
    pub elapsed: Duration,
    pub counters: Counters,
    pub throughput_cmd_per_sec: f64,
    pub traffic: TrafficRate,
    pub latency: LatencyStats,
    pub frames: u64,
    pub heartbeats: u64,
    pub ack: AckLatency,
}
