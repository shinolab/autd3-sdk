use crate::frame::REPLY_DATA_BYTES_MAX;

crate::wire_enum_u8! {
    #[derive(enum_map::Enum)]
    pub enum Telemetry {
        FifoDrop = 0x00,
        Dedup = 0x01,
        SeqMismatch = 0x02,
        DispatchError = 0x03,
        Processed = 0x04,
        Failsafe = 0x05,
        SyncResync = 0x06,
        PtpUnlockFailsafe = 0x07,
    }
}

impl Telemetry {
    pub const COUNTER_BYTES: usize = 4;
    pub const REPLY_BYTES: usize = Self::ALL.len() * Self::COUNTER_BYTES;
}

const _: () = assert!(Telemetry::REPLY_BYTES <= REPLY_DATA_BYTES_MAX);
