pub use autd3_cpu_wire::Telemetry;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TelemetryCounters {
    counters: [u32; Telemetry::ALL.len()],
}

impl TelemetryCounters {
    pub(crate) fn parse(value: &[u8]) -> Option<Self> {
        if value.len() < Telemetry::REPLY_BYTES {
            return None;
        }
        let mut counters = [0u32; Telemetry::ALL.len()];
        for (counter, chunk) in counters
            .iter_mut()
            .zip(value.as_chunks::<{ Telemetry::COUNTER_BYTES }>().0)
        {
            *counter = u32::from_le_bytes(*chunk);
        }
        Some(Self { counters })
    }

    #[must_use]
    pub fn get(&self, counter: Telemetry) -> u32 {
        self.counters
            .get(usize::from(counter.as_u8()))
            .copied()
            .unwrap_or(0)
    }
}

impl core::ops::Deref for TelemetryCounters {
    type Target = [u32; Telemetry::ALL.len()];

    fn deref(&self) -> &Self::Target {
        &self.counters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_every_counter_in_id_order() {
        let mut bytes = std::vec::Vec::new();
        for id in (0u32..).take(Telemetry::ALL.len()) {
            bytes.extend_from_slice(&(id * 1000 + 1).to_le_bytes());
        }
        let counters = TelemetryCounters::parse(&bytes).unwrap();
        assert_eq!(counters.get(Telemetry::FifoDrop), 1);
        assert_eq!(counters.get(Telemetry::SyncResync), 6001);
    }

    #[test]
    fn a_short_reply_is_rejected() {
        assert!(TelemetryCounters::parse(&[0; 4]).is_none());
    }
}
