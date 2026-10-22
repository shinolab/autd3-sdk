use autd3_cpu_wire::udp::Flags;

pub(crate) const SYNCED_FLAGS: Flags = Flags::PTP_LOCKED.union(Flags::SYNC_READY);
pub(crate) const PTP_FLAGS: Flags = Flags::PTP_LOCKED.union(Flags::PTP_FAILSAFE);
