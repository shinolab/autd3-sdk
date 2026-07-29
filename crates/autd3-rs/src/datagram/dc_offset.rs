use crate::link::DcClock;

#[derive(Debug, Clone)]
pub(crate) enum DcOffset {
    Fixed(i64),
    Clock(DcClock),
}

impl DcOffset {
    pub(crate) fn offset_ns(&self) -> i64 {
        match self {
            Self::Fixed(offset_ns) => *offset_ns,
            Self::Clock(clock) => clock.offset_ns().unwrap_or(0),
        }
    }
}

impl From<Option<DcClock>> for DcOffset {
    fn from(clock: Option<DcClock>) -> Self {
        clock.map_or(Self::Fixed(0), Self::Clock)
    }
}
