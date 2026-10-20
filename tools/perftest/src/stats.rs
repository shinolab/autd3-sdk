use std::collections::BTreeMap;
use std::time::Duration;

use autd3_cpu_wire::udp::{FrameReply, Header};
use autd3_rs::Frame;
use autd3_rs::protocol::FrameHeader;

const LINK_BITS_PER_SEC: f64 = 100_000_000.0;
const WIRE_OVERHEAD_BYTES: u64 = 8 + 40 + 14 + 4 + 8 + 12;

const SUB_BUCKET_BITS: u32 = 7;
const SUB_BUCKETS: usize = 1 << SUB_BUCKET_BITS;
const BUCKETS: usize = (u64::BITS - SUB_BUCKET_BITS + 1) as usize * SUB_BUCKETS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SampleStatus {
    Ok,
    DeviceError(u8),
    Timeout,
    NetworkError,
}

#[derive(Debug, Default)]
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct Traffic {
    devices: usize,
    tx_datagrams: u64,
    tx_bytes: u64,
    rx_datagrams: u64,
    rx_bytes: u64,
}

impl Traffic {
    pub(crate) const fn new(devices: usize) -> Self {
        Self {
            devices,
            tx_datagrams: 0,
            tx_bytes: 0,
            rx_datagrams: 0,
            rx_bytes: 0,
        }
    }

    pub(crate) fn sent(&mut self, frame: &Frame<'_>) {
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TrafficRate {
    pub tx_byte_per_sec: f64,
    pub line_down_percent: f64,
    pub line_up_percent: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LatencyStats {
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
}

pub(crate) struct Histogram {
    buckets: Vec<u64>,
    count: u64,
    sum_ns: u128,
    min_ns: u64,
    max_ns: u64,
}

impl Histogram {
    pub(crate) fn new() -> Self {
        Self {
            buckets: vec![0; BUCKETS],
            count: 0,
            sum_ns: 0,
            min_ns: u64::MAX,
            max_ns: 0,
        }
    }

    pub(crate) fn record(&mut self, rtt: Duration) {
        let ns = u64::try_from(rtt.as_nanos()).unwrap_or(u64::MAX);
        self.buckets[bucket_index(ns)] += 1;
        self.count += 1;
        self.sum_ns += u128::from(ns);
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
    }

    pub(crate) fn clear(&mut self) {
        self.buckets.fill(0);
        self.count = 0;
        self.sum_ns = 0;
        self.min_ns = u64::MAX;
        self.max_ns = 0;
    }

    pub(crate) const fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn latency(&self) -> LatencyStats {
        if self.count == 0 {
            return LatencyStats::default();
        }
        let mean_ns = u64::try_from(self.sum_ns / u128::from(self.count)).unwrap_or(u64::MAX);
        LatencyStats {
            mean: Duration::from_nanos(mean_ns),
            p50: self.percentile(50),
            p95: self.percentile(95),
            p99: self.percentile(99),
            min: Duration::from_nanos(self.min_ns),
            max: Duration::from_nanos(self.max_ns),
        }
    }

    fn percentile(&self, p: u64) -> Duration {
        let rank = p
            .saturating_mul(self.count)
            .saturating_add(99)
            .saturating_div(100)
            .max(1);
        let mut seen = 0u64;
        for (index, &n) in self.buckets.iter().enumerate() {
            seen += n;
            if seen >= rank {
                let ns = bucket_upper_ns(index).clamp(self.min_ns, self.max_ns);
                return Duration::from_nanos(ns);
            }
        }
        Duration::from_nanos(self.max_ns)
    }
}

fn bucket_index(ns: u64) -> usize {
    let sub_buckets = SUB_BUCKETS as u64;
    let index = if ns < sub_buckets {
        ns
    } else {
        let shift = u64::from(u64::BITS - 1 - ns.leading_zeros() - SUB_BUCKET_BITS);
        (shift + 1) * sub_buckets + (ns >> shift) - sub_buckets
    };
    usize::try_from(index).unwrap_or(BUCKETS - 1)
}

const fn bucket_upper_ns(index: usize) -> u64 {
    let sub_buckets = SUB_BUCKETS as u64;
    let group = index as u64 / sub_buckets;
    let mantissa = index as u64 % sub_buckets;
    if group == 0 {
        return mantissa;
    }
    let shift = group - 1;
    ((sub_buckets + mantissa) << shift) + ((1u64 << shift) - 1)
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DriverAckLatency {
    pub frames: u64,
    pub mean: Duration,
    pub worst: Duration,
}

#[derive(Debug)]
pub(crate) struct Summary {
    pub elapsed: Duration,
    pub counters: Counters,
    pub recoveries: u64,
    pub throughput_cmd_per_sec: f64,
    pub traffic: TrafficRate,
    pub latency: LatencyStats,
    pub driver_ack: Option<DriverAckLatency>,
}
