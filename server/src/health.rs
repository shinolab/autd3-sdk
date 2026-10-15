use autd3_cpu_wire::udp::{Flags, UnitInfo};

pub const MISSES_BEFORE_SILENT: u32 = 3;
const SYNCED: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Silent(usize),
    Answering(usize),
    Unlocked(usize),
    Locked(usize),
}

#[derive(Clone, Debug)]
pub struct Health {
    known: Vec<Option<UnitInfo>>,
    misses: Vec<u32>,
}

fn is_synced(info: Option<&UnitInfo>) -> Option<bool> {
    info.map(|info| info.flags.contains(SYNCED))
}

impl Health {
    #[must_use]
    pub fn new(devices: Vec<Option<UnitInfo>>) -> Self {
        Self {
            misses: devices
                .iter()
                .map(|device| {
                    if device.is_some() {
                        0
                    } else {
                        MISSES_BEFORE_SILENT
                    }
                })
                .collect(),
            known: devices,
        }
    }

    #[must_use]
    pub fn known(&self) -> &[Option<UnitInfo>] {
        &self.known
    }

    #[must_use]
    pub fn any_silent(&self) -> bool {
        self.misses
            .iter()
            .any(|&misses| misses >= MISSES_BEFORE_SILENT)
    }

    pub fn observe(&mut self, devices: &[Option<UnitInfo>]) -> Vec<Change> {
        let mut changes = Vec::new();
        for (index, device) in devices.iter().enumerate().take(self.known.len()) {
            let answered = device.filter(|info| info.flags.contains(Flags::ASSIGNED));
            let Some(info) = answered else {
                self.misses[index] = self.misses[index].saturating_add(1);
                if self.misses[index] == MISSES_BEFORE_SILENT {
                    self.known[index] = None;
                    changes.push(Change::Silent(index));
                }
                continue;
            };
            if self.misses[index] >= MISSES_BEFORE_SILENT {
                changes.push(Change::Answering(index));
            }
            self.misses[index] = 0;
            let was_synced = is_synced(self.known[index].as_ref());
            let synced = info.flags.contains(SYNCED);
            if was_synced.is_some_and(|was_synced| was_synced != synced) {
                changes.push(if synced {
                    Change::Locked(index)
                } else {
                    Change::Unlocked(index)
                });
            }
            self.known[index] = Some(info);
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use zerocopy::little_endian::{I32, U64};

    use super::*;

    const NOTHING: [Change; 0] = [];

    fn device(flags: Flags) -> UnitInfo {
        UnitInfo {
            unit_id: 0,
            flags: flags | Flags::ASSIGNED,
            upstream_port: 0,
            reserved: 0,
            fw_version: [0, 10, 1],
            reserved2: 0,
            sys_time: U64::new(0),
            ptp_offset_ns: I32::new(0),
        }
    }

    #[test]
    fn a_device_turns_silent_only_after_consecutive_misses() {
        let mut health = Health::new(vec![Some(device(SYNCED)), Some(device(SYNCED))]);
        for _ in 1..MISSES_BEFORE_SILENT {
            assert_eq!(health.observe(&[Some(device(SYNCED)), None]), NOTHING);
            assert!(health.known()[1].is_some());
            assert!(!health.any_silent());
        }
        assert_eq!(
            health.observe(&[Some(device(SYNCED)), None]),
            [Change::Silent(1)]
        );
        assert!(health.known()[1].is_none());
        assert!(health.any_silent());
        assert_eq!(health.observe(&[Some(device(SYNCED)), None]), NOTHING);
    }

    #[test]
    fn an_answer_in_between_restarts_the_miss_count() {
        let mut health = Health::new(vec![Some(device(SYNCED))]);
        for _ in 0..MISSES_BEFORE_SILENT * 2 {
            assert_eq!(health.observe(&[None]), NOTHING);
            assert_eq!(health.observe(&[Some(device(SYNCED))]), NOTHING);
        }
        assert!(!health.any_silent());
    }

    #[test]
    fn a_silent_device_that_answers_again_is_reported_once() {
        let mut health = Health::new(vec![None]);
        assert!(health.any_silent());
        assert_eq!(
            health.observe(&[Some(device(SYNCED))]),
            [Change::Answering(0)]
        );
        assert!(!health.any_silent());
        assert_eq!(health.observe(&[Some(device(SYNCED))]), NOTHING);
    }

    #[test]
    fn a_device_that_lost_its_id_counts_as_a_miss() {
        let mut health = Health::new(vec![Some(device(SYNCED))]);
        let unassigned = Some(device(SYNCED)).map(|info| UnitInfo {
            flags: SYNCED,
            ..info
        });
        for _ in 1..MISSES_BEFORE_SILENT {
            assert_eq!(health.observe(&[unassigned]), NOTHING);
        }
        assert_eq!(health.observe(&[unassigned]), [Change::Silent(0)]);
    }

    #[test]
    fn losing_and_regaining_the_lock_are_reported() {
        let mut health = Health::new(vec![Some(device(SYNCED))]);
        assert_eq!(
            health.observe(&[Some(device(Flags::PTP_LOCKED))]),
            [Change::Unlocked(0)]
        );
        assert_eq!(health.observe(&[Some(device(Flags::empty()))]), NOTHING);
        assert_eq!(health.observe(&[Some(device(SYNCED))]), [Change::Locked(0)]);
    }
}
