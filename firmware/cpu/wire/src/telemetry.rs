crate::wire_enum! {
    pub enum Telemetry {
        FifoDrop = 0x00,
        Dedup = 0x01,
        SeqMismatch = 0x02,
        DispatchError = 0x03,
        Processed = 0x04,
        Failsafe = 0x05,
        SyncResync = 0x06,
    }
}

impl Telemetry {
    pub const CPU_COUNTER_COUNT: usize = Self::SyncResync as usize;
}

const _: () = assert!(Telemetry::ALL.len() == Telemetry::CPU_COUNTER_COUNT + 1);
