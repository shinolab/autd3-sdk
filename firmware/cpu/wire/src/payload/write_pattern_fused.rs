use zerocopy::little_endian::U64;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{TransitionModeKind, try_read_header};
use crate::layout::PATTERN_RAW_DATA_LEN;
use crate::value::TransitionMode;
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct WritePatternFusedPayload {
    pub bank: PatternBank,
    pub transition_mode: TransitionModeKind,
    pub transition_value: U64,
}

impl WritePatternFusedPayload {
    #[must_use]
    pub const fn new(bank: PatternBank, transition: TransitionMode) -> Self {
        Self {
            bank,
            transition_mode: transition.kind(),
            transition_value: U64::new(transition.value()),
        }
    }

    pub fn parse(payload: &[u8]) -> Result<(Self, &[u8; PATTERN_RAW_DATA_LEN]), Error> {
        let (p, rest) = try_read_header::<Self>(payload)?;
        let data = rest.try_into().map_err(|_| Error::InvalidPayload)?;
        if !matches!(
            p.transition_mode,
            TransitionModeKind::Immediate | TransitionModeKind::Ext
        ) {
            return Err(Error::InvalidTransitionMode);
        }
        Ok((p, data))
    }
}

assert_layout!(WritePatternFusedPayload {
    bank: 0,
    transition_mode: 1,
    transition_value: 2,
} size 10);
const _: () = assert!(
    core::mem::size_of::<WritePatternFusedPayload>() + PATTERN_RAW_DATA_LEN
        <= crate::frame::PAYLOAD_BYTES
);

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;
    use crate::frame::PAYLOAD_BYTES;
    use crate::value::{GpioIn, SysTime};

    const HEADER: usize = core::mem::size_of::<WritePatternFusedPayload>();

    struct Frame {
        bytes: [u8; PAYLOAD_BYTES],
        len: usize,
    }

    impl core::ops::Deref for Frame {
        type Target = [u8];

        fn deref(&self) -> &[u8] {
            &self.bytes[..self.len]
        }
    }

    impl core::ops::DerefMut for Frame {
        fn deref_mut(&mut self) -> &mut [u8] {
            &mut self.bytes[..self.len]
        }
    }

    fn frame(header: &WritePatternFusedPayload, data_len: usize) -> Frame {
        let mut bytes = [0u8; PAYLOAD_BYTES];
        bytes[..HEADER].copy_from_slice(header.as_bytes());
        for (i, b) in bytes[HEADER..HEADER + data_len].iter_mut().enumerate() {
            *b = i as u8;
        }
        Frame {
            bytes,
            len: HEADER + data_len,
        }
    }

    #[rstest]
    #[case(TransitionMode::Immediate, TransitionModeKind::Immediate)]
    #[case(TransitionMode::Ext, TransitionModeKind::Ext)]
    fn a_built_payload_passes_parse(
        #[case] transition: TransitionMode,
        #[case] mode: TransitionModeKind,
    ) {
        let built = WritePatternFusedPayload::new(PatternBank::B1, transition);
        let bytes = frame(&built, PATTERN_RAW_DATA_LEN);
        let (parsed, data) = WritePatternFusedPayload::parse(&bytes).unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(parsed.transition_mode, mode);
        assert_eq!(parsed.transition_value.get(), 0);
        assert_eq!(&data[..], &bytes[HEADER..]);
    }

    #[rstest]
    #[case(PATTERN_RAW_DATA_LEN - 1)]
    #[case(PATTERN_RAW_DATA_LEN + 1)]
    #[case(0)]
    fn a_payload_of_the_wrong_length_is_rejected(#[case] data_len: usize) {
        let built = WritePatternFusedPayload::new(PatternBank::B0, TransitionMode::Immediate);
        assert_eq!(
            WritePatternFusedPayload::parse(&frame(&built, data_len)).err(),
            Some(Error::InvalidPayload)
        );
    }

    #[test]
    fn an_unknown_bank_or_mode_is_an_invalid_payload() {
        let built = WritePatternFusedPayload::new(PatternBank::B0, TransitionMode::Immediate);
        let mut bytes = frame(&built, PATTERN_RAW_DATA_LEN);
        bytes[0] = 2;
        assert_eq!(
            WritePatternFusedPayload::parse(&bytes).err(),
            Some(Error::InvalidPayload)
        );
        let mut bytes = frame(&built, PATTERN_RAW_DATA_LEN);
        bytes[1] = 0x03;
        assert_eq!(
            WritePatternFusedPayload::parse(&bytes).err(),
            Some(Error::InvalidPayload)
        );
    }

    #[rstest]
    #[case(TransitionMode::SyncIdx)]
    #[case(TransitionMode::SysTime { time: SysTime::from_nanos(0x0123_4567_89AB) })]
    #[case(TransitionMode::Gpio(GpioIn::I1))]
    fn a_transition_needing_a_finite_loop_is_rejected(#[case] transition: TransitionMode) {
        let built = WritePatternFusedPayload::new(PatternBank::B0, transition);
        assert_eq!(
            WritePatternFusedPayload::parse(&frame(&built, PATTERN_RAW_DATA_LEN)).err(),
            Some(Error::InvalidTransitionMode)
        );
    }

    #[test]
    fn a_wrong_length_is_reported_before_the_transition_mode() {
        let built = WritePatternFusedPayload::new(PatternBank::B0, TransitionMode::SyncIdx);
        assert_eq!(
            WritePatternFusedPayload::parse(&frame(&built, PATTERN_RAW_DATA_LEN - 1)).err(),
            Some(Error::InvalidPayload)
        );
    }
}
