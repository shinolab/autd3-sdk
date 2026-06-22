use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleStatus {
    Ok,
    DeviceError(u8),
    Timeout,
    LinkError,
}

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub index: u64,
    pub rtt: Duration,
    pub status: SampleStatus,
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

#[derive(Debug)]
pub struct Summary {
    pub elapsed: Duration,
    pub success: u64,
    pub device_errors: BTreeMap<u8, u64>,
    pub timeouts: u64,
    pub link_errors: u64,
    pub stale_cycles: u64,
    pub lost_cycles: u64,
    pub throughput_cmd_per_sec: f64,
    pub throughput_byte_per_sec: f64,
    pub latency: LatencyStats,
}

impl Summary {
    pub fn from_samples(
        samples: &[Sample],
        frame_bytes: usize,
        elapsed: Duration,
        stale_cycles: u64,
        lost_cycles: u64,
    ) -> Self {
        let mut device_errors: BTreeMap<u8, u64> = BTreeMap::new();
        let mut timeouts = 0u64;
        let mut link_errors = 0u64;
        let mut oks_ns: Vec<u64> = Vec::with_capacity(samples.len());

        for s in samples {
            match s.status {
                SampleStatus::Ok => oks_ns.push(duration_to_nanos(s.rtt)),
                SampleStatus::DeviceError(code) => {
                    *device_errors.entry(code).or_insert(0) += 1;
                }
                SampleStatus::Timeout => timeouts += 1,
                SampleStatus::LinkError => link_errors += 1,
            }
        }

        let success = oks_ns.len() as u64;
        let latency = latency_stats(&mut oks_ns);

        let secs = elapsed.as_secs_f64();
        let (cmd_per_sec, byte_per_sec) = if secs > 0.0 {
            let cmds = success as f64 / secs;
            let bytes = (success * frame_bytes as u64) as f64 / secs;
            (cmds, bytes)
        } else {
            (0.0, 0.0)
        };

        Self {
            elapsed,
            success,
            device_errors,
            timeouts,
            link_errors,
            stale_cycles,
            lost_cycles,
            throughput_cmd_per_sec: cmd_per_sec,
            throughput_byte_per_sec: byte_per_sec,
            latency,
        }
    }
}

fn duration_to_nanos(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

fn latency_stats(values: &mut [u64]) -> LatencyStats {
    if values.is_empty() {
        return LatencyStats::default();
    }
    values.sort_unstable();

    let sum: u128 = values.iter().map(|&v| u128::from(v)).sum();
    let n_u128 = values.len() as u128;
    let mean_ns = u64::try_from(sum / n_u128).unwrap_or(u64::MAX);

    LatencyStats {
        mean: Duration::from_nanos(mean_ns),
        p50: Duration::from_nanos(values[nearest_rank_idx(values.len(), 50)]),
        p95: Duration::from_nanos(values[nearest_rank_idx(values.len(), 95)]),
        p99: Duration::from_nanos(values[nearest_rank_idx(values.len(), 99)]),
        min: Duration::from_nanos(values[0]),
        max: Duration::from_nanos(values[values.len() - 1]),
    }
}

fn nearest_rank_idx(n: usize, p: usize) -> usize {
    let raw = p.saturating_mul(n).saturating_add(99).saturating_div(100);
    raw.saturating_sub(1).min(n - 1)
}
