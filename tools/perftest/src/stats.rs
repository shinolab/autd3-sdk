use std::collections::BTreeMap;
use std::time::Duration;

const SUB_BUCKET_BITS: u32 = 7;
const SUB_BUCKETS: usize = 1 << SUB_BUCKET_BITS;
const BUCKETS: usize = (u64::BITS - SUB_BUCKET_BITS + 1) as usize * SUB_BUCKETS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleStatus {
    Ok,
    DeviceError(u8),
    Timeout,
    NetworkError,
}

#[derive(Debug, Default)]
pub struct Counters {
    pub ok: u64,
    pub timeouts: u64,
    pub device_errors: BTreeMap<u8, u64>,
    pub network_errors: u64,
}

impl Counters {
    pub fn observe(&mut self, status: SampleStatus) {
        match status {
            SampleStatus::Ok => self.ok += 1,
            SampleStatus::Timeout => self.timeouts += 1,
            SampleStatus::NetworkError => self.network_errors += 1,
            SampleStatus::DeviceError(code) => *self.device_errors.entry(code).or_insert(0) += 1,
        }
    }

    pub fn device_error_total(&self) -> u64 {
        self.device_errors.values().sum()
    }

    pub fn total(&self) -> u64 {
        self.ok + self.timeouts + self.network_errors + self.device_error_total()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LatencyStats {
    pub mean: Duration,
    pub p50: Duration,
    pub p95: Duration,
    pub p99: Duration,
    pub min: Duration,
    pub max: Duration,
}

pub struct Histogram {
    buckets: Vec<u64>,
    count: u64,
    sum_ns: u128,
    min_ns: u64,
    max_ns: u64,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            buckets: vec![0; BUCKETS],
            count: 0,
            sum_ns: 0,
            min_ns: u64::MAX,
            max_ns: 0,
        }
    }

    pub fn record(&mut self, rtt: Duration) {
        let ns = u64::try_from(rtt.as_nanos()).unwrap_or(u64::MAX);
        self.buckets[bucket_index(ns)] += 1;
        self.count += 1;
        self.sum_ns += u128::from(ns);
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
    }

    pub fn clear(&mut self) {
        self.buckets.fill(0);
        self.count = 0;
        self.sum_ns = 0;
        self.min_ns = u64::MAX;
        self.max_ns = 0;
    }

    pub const fn count(&self) -> u64 {
        self.count
    }

    pub fn latency(&self) -> LatencyStats {
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
pub struct DriverAckLatency {
    pub frames: u64,
    pub mean: Duration,
    pub worst: Duration,
}

#[derive(Debug)]
pub struct Summary {
    pub elapsed: Duration,
    pub counters: Counters,
    pub missed_replies: u64,
    pub throughput_cmd_per_sec: f64,
    pub throughput_byte_per_sec: f64,
    pub latency: LatencyStats,
    pub driver_ack: Option<DriverAckLatency>,
}
