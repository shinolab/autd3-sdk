use zerocopy::little_endian::{I16, U16};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::frame::REPLY_DATA_BYTES_MAX;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ReadSyncPsStatsPayload {
    pub clear: bool,
}

impl ReadSyncPsStatsPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

assert_layout!(ReadSyncPsStatsPayload { clear: 0 } size 1);

#[derive(
    FromBytes,
    IntoBytes,
    KnownLayout,
    Immutable,
    Unaligned,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    Default,
)]
#[repr(C)]
pub struct SyncPsStats {
    pub diff_min: I16,
    pub diff_max: I16,
    pub diff_outliers: U16,
    pub rate_min: I16,
    pub rate_max: I16,
    pub fine_exits: U16,
}

assert_layout!(SyncPsStats {
    diff_min: 0,
    diff_max: 2,
    diff_outliers: 4,
    rate_min: 6,
    rate_max: 8,
    fine_exits: 10,
} size 12);
const _: () = assert!(size_of::<SyncPsStats>() <= REPLY_DATA_BYTES_MAX);

crate::wire_enum_u8! {
    pub enum TdcSource {
        SyncIn = 0,
        Calibration = 1,
    }
}

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct StartTdcCapturePayload {
    pub source: TdcSource,
}

impl StartTdcCapturePayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        try_read_exact(payload)
    }
}

assert_layout!(StartTdcCapturePayload { source: 0 } size 1);

pub const TDC_SAMPLES: usize = 128;
pub const TDC_CHUNK_SAMPLES: usize = 16;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ReadTdcPayload {
    pub chunk: u8,
}

impl ReadTdcPayload {
    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p: Self = try_read_exact(payload)?;
        if usize::from(p.chunk) >= TDC_SAMPLES / TDC_CHUNK_SAMPLES {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }
}

assert_layout!(ReadTdcPayload { chunk: 0 } size 1);

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned, Clone, Copy, Debug)]
#[repr(C)]
pub struct TdcChunk {
    pub capturing: bool,
    pub locked: bool,
    pub samples: [U16; TDC_CHUNK_SAMPLES],
}

assert_layout!(TdcChunk {
    capturing: 0,
    locked: 1,
    samples: 2,
} size 2 + 2 * TDC_CHUNK_SAMPLES);
const _: () = assert!(size_of::<TdcChunk>() <= REPLY_DATA_BYTES_MAX);
const _: () = assert!(TDC_SAMPLES.is_multiple_of(TDC_CHUNK_SAMPLES));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tdc_accepts_only_chunks_inside_the_buffer() {
        assert_eq!(ReadTdcPayload::parse(&[7]).map(|p| p.chunk), Ok(7));
        assert_eq!(
            ReadTdcPayload::parse(&[8]).map(|p| p.chunk),
            Err(Error::InvalidPayload)
        );
        assert_eq!(
            ReadTdcPayload::parse(&[0, 0]).map(|p| p.chunk),
            Err(Error::InvalidPayload)
        );
    }

    #[test]
    fn start_tdc_capture_rejects_an_unknown_source() {
        assert_eq!(
            StartTdcCapturePayload::parse(&[1]).map(|p| p.source),
            Ok(TdcSource::Calibration)
        );
        assert_eq!(
            StartTdcCapturePayload::parse(&[2]).map(|p| p.source),
            Err(Error::InvalidPayload)
        );
    }
}
