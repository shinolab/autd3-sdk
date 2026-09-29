use autd3_rs_core::DeviceClock;

#[derive(Debug, Clone)]
pub(crate) enum ClockOffset {
    Fixed(i64),
    Clock(DeviceClock),
}

impl ClockOffset {
    pub(crate) fn offset_ns(&self) -> i64 {
        match self {
            Self::Fixed(offset_ns) => *offset_ns,
            Self::Clock(clock) => clock.offset_ns().unwrap_or(0),
        }
    }
}

impl From<Option<DeviceClock>> for ClockOffset {
    fn from(clock: Option<DeviceClock>) -> Self {
        clock.map_or(Self::Fixed(0), Self::Clock)
    }
}
