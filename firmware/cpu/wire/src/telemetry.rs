crate::wire_enum! {
    pub enum Telemetry {
        FifoDrop = 0x00,
        Dedup = 0x01,
        SeqMismatch = 0x02,
        DispatchError = 0x03,
        Processed = 0x04,
        Failsafe = 0x05,
    }
}

impl Telemetry {
    pub const COUNT: usize = 6;
}
