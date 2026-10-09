use core::num::{NonZeroU8, NonZeroU16};

use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_exact};
use crate::fpga_params::{EMISSION_MAX_INDICES, EmissionType, NUM_FOCI_MAX, REP_INFINITE};
use crate::layout::{BUFFER_SIZE_MIN, MAX_FOCI_TOTAL};
use crate::{Error, PatternBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigPatternPayload {
    pub bank: PatternBank,
    pub emission_type: EmissionType,
    pub divider: U16,
    pub size: U32,
    pub num_foci: Option<NonZeroU8>,
    pub reserved: u8,
    pub sound_speed: U16,
    pub rep: U16,
}

const SOUND_SPEED_UNITS_PER_M_S: f32 = 64.0;

impl ConfigPatternPayload {
    pub fn raw(
        bank: PatternBank,
        divider: NonZeroU16,
        size: usize,
        rep: u16,
    ) -> Result<Self, PayloadBuildError> {
        if !(1..=EMISSION_MAX_INDICES as usize).contains(&size) {
            return Err(PayloadBuildError::StmSizeOutOfRange {
                size,
                min: 1,
                max: EMISSION_MAX_INDICES as usize,
            });
        }
        if size < BUFFER_SIZE_MIN && rep != REP_INFINITE {
            return Err(PayloadBuildError::FiniteLoopNeedsMultipleSamples { size });
        }
        Ok(Self {
            bank,
            emission_type: EmissionType::Raw,
            divider: U16::new(divider.get()),
            size: U32::new(size as u32),
            num_foci: None,
            reserved: 0,
            sound_speed: U16::new(0),
            rep: U16::new(rep),
        })
    }

    #[allow(clippy::cast_sign_loss)]
    pub fn foci(
        bank: PatternBank,
        divider: NonZeroU16,
        size: usize,
        num_foci: u8,
        sound_speed_m_s: f32,
        rep: u16,
    ) -> Result<Self, PayloadBuildError> {
        if size < BUFFER_SIZE_MIN {
            return Err(PayloadBuildError::PatternSizeTooSmall {
                size,
                min: BUFFER_SIZE_MIN,
            });
        }
        let Some(foci_per_index) = NonZeroU8::new(num_foci).filter(|n| n.get() <= NUM_FOCI_MAX)
        else {
            return Err(PayloadBuildError::NumFociOutOfRange {
                num_foci,
                max: NUM_FOCI_MAX,
            });
        };
        if size > MAX_FOCI_TOTAL / usize::from(num_foci) {
            return Err(PayloadBuildError::StmFociExceedCapacity {
                size,
                num_foci,
                capacity: MAX_FOCI_TOTAL,
            });
        }
        let sound_speed = sound_speed_m_s * SOUND_SPEED_UNITS_PER_M_S + 0.5;
        if sound_speed >= f32::from(u16::MAX) + 1.0 {
            return Err(PayloadBuildError::SoundSpeedTooLarge {
                m_s: sound_speed_m_s,
                max: f32::from(u16::MAX) / SOUND_SPEED_UNITS_PER_M_S,
            });
        }
        let Some(sound_speed) = NonZeroU16::new(sound_speed as u16) else {
            return Err(PayloadBuildError::SoundSpeedZero);
        };
        Ok(Self {
            bank,
            emission_type: EmissionType::Foci,
            divider: U16::new(divider.get()),
            size: U32::new(size as u32),
            num_foci: Some(foci_per_index),
            reserved: 0,
            sound_speed: U16::new(sound_speed.get()),
            rep: U16::new(rep),
        })
    }

    #[must_use]
    pub const fn divider(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.divider.get())
    }

    #[must_use]
    pub const fn sound_speed(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.sound_speed.get())
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        p.divider().ok_or(Error::InvalidPayload)?;
        let size = p.size.get();
        let emission_valid = match p.emission_type {
            EmissionType::Raw => {
                (1..=EMISSION_MAX_INDICES).contains(&size)
                    && (size >= BUFFER_SIZE_MIN as u32 || p.rep.get() == REP_INFINITE)
            }
            EmissionType::Foci => p
                .num_foci
                .zip(p.sound_speed())
                .is_some_and(|(num_foci, _)| {
                    num_foci.get() <= NUM_FOCI_MAX
                        && (BUFFER_SIZE_MIN as u32
                            ..=MAX_FOCI_TOTAL as u32 / u32::from(num_foci.get()))
                            .contains(&size)
                }),
        };
        if !emission_valid {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, bank) == 0);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, emission_type) == 1);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, divider) == 2);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, size) == 4);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, num_foci) == 8);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, sound_speed) == 10);
const _: () = assert!(core::mem::offset_of!(ConfigPatternPayload, rep) == 12);
const _: () = assert!(core::mem::size_of::<ConfigPatternPayload>() == 14);

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    const DIVIDER: NonZeroU16 = NonZeroU16::new(10).unwrap();
    const SOUND_SPEED_M_S: f32 = 340.0;
    const MAX_INDICES: usize = EMISSION_MAX_INDICES as usize;

    #[rstest]
    #[case(1, REP_INFINITE)]
    #[case(BUFFER_SIZE_MIN, 0)]
    #[case(MAX_INDICES, 3)]
    fn a_built_raw_payload_passes_parse(#[case] size: usize, #[case] rep: u16) {
        let built = ConfigPatternPayload::raw(PatternBank::B1, DIVIDER, size, rep).unwrap();
        let parsed = ConfigPatternPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.bank, PatternBank::B1);
        assert_eq!(parsed.emission_type, EmissionType::Raw);
        assert_eq!(parsed.divider.get(), 10);
        assert_eq!(parsed.size.get() as usize, size);
        assert_eq!(parsed.num_foci, None);
        assert_eq!(parsed.sound_speed.get(), 0);
        assert_eq!(parsed.rep.get(), rep);
    }

    #[rstest]
    #[case(0)]
    #[case(MAX_INDICES + 1)]
    #[case(usize::MAX)]
    fn a_raw_size_outside_the_bank_is_rejected(#[case] size: usize) {
        assert_eq!(
            ConfigPatternPayload::raw(PatternBank::B0, DIVIDER, size, REP_INFINITE).err(),
            Some(PayloadBuildError::StmSizeOutOfRange {
                size,
                min: 1,
                max: MAX_INDICES,
            })
        );
    }

    #[test]
    fn a_single_raw_index_needs_an_infinite_loop() {
        assert_eq!(
            ConfigPatternPayload::raw(PatternBank::B0, DIVIDER, 1, 0).err(),
            Some(PayloadBuildError::FiniteLoopNeedsMultipleSamples { size: 1 })
        );
    }

    #[rstest]
    #[case(BUFFER_SIZE_MIN, 1)]
    #[case(MAX_FOCI_TOTAL, 1)]
    #[case(MAX_FOCI_TOTAL / usize::from(NUM_FOCI_MAX), NUM_FOCI_MAX)]
    fn a_built_foci_payload_passes_parse(#[case] size: usize, #[case] num_foci: u8) {
        let built = ConfigPatternPayload::foci(
            PatternBank::B1,
            DIVIDER,
            size,
            num_foci,
            SOUND_SPEED_M_S,
            7,
        )
        .unwrap();
        let parsed = ConfigPatternPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.emission_type, EmissionType::Foci);
        assert_eq!(parsed.size.get() as usize, size);
        assert_eq!(parsed.num_foci.map(NonZeroU8::get), Some(num_foci));
        assert_eq!(parsed.sound_speed.get(), 340 * 64);
        assert_eq!(parsed.rep.get(), 7);
    }

    #[rstest]
    #[case(
        BUFFER_SIZE_MIN - 1,
        1,
        PayloadBuildError::PatternSizeTooSmall { size: BUFFER_SIZE_MIN - 1, min: BUFFER_SIZE_MIN }
    )]
    #[case(
        BUFFER_SIZE_MIN,
        0,
        PayloadBuildError::NumFociOutOfRange { num_foci: 0, max: NUM_FOCI_MAX }
    )]
    #[case(
        BUFFER_SIZE_MIN,
        NUM_FOCI_MAX + 1,
        PayloadBuildError::NumFociOutOfRange { num_foci: NUM_FOCI_MAX + 1, max: NUM_FOCI_MAX }
    )]
    #[case(
        MAX_FOCI_TOTAL + 1,
        1,
        PayloadBuildError::StmFociExceedCapacity {
            size: MAX_FOCI_TOTAL + 1,
            num_foci: 1,
            capacity: MAX_FOCI_TOTAL,
        }
    )]
    #[case(
        MAX_FOCI_TOTAL / usize::from(NUM_FOCI_MAX) + 1,
        NUM_FOCI_MAX,
        PayloadBuildError::StmFociExceedCapacity {
            size: MAX_FOCI_TOTAL / usize::from(NUM_FOCI_MAX) + 1,
            num_foci: NUM_FOCI_MAX,
            capacity: MAX_FOCI_TOTAL,
        }
    )]
    fn an_invalid_foci_config_is_rejected(
        #[case] size: usize,
        #[case] num_foci: u8,
        #[case] expected: PayloadBuildError,
    ) {
        assert_eq!(
            ConfigPatternPayload::foci(
                PatternBank::B0,
                DIVIDER,
                size,
                num_foci,
                SOUND_SPEED_M_S,
                0
            )
            .err(),
            Some(expected)
        );
    }

    #[rstest]
    #[case(1.0 / 64.0, 1)]
    #[case(0.5 / 64.0, 1)]
    #[case(340.0, 21760)]
    #[case(65535.0 / 64.0, u16::MAX)]
    #[case(65535.4 / 64.0, u16::MAX)]
    fn a_sound_speed_is_carried_in_units_of_one_64th_m_s(#[case] m_s: f32, #[case] units: u16) {
        let built = ConfigPatternPayload::foci(PatternBank::B0, DIVIDER, 2, 1, m_s, 0).unwrap();
        assert_eq!(built.sound_speed.get(), units);
        assert!(ConfigPatternPayload::parse(built.as_bytes()).is_ok());
    }

    #[rstest]
    #[case(0.0)]
    #[case(0.4 / 64.0)]
    #[case(-340.0)]
    #[case(f32::NAN)]
    fn a_sound_speed_that_rounds_to_zero_is_rejected(#[case] m_s: f32) {
        assert_eq!(
            ConfigPatternPayload::foci(PatternBank::B0, DIVIDER, 2, 1, m_s, 0).err(),
            Some(PayloadBuildError::SoundSpeedZero)
        );
    }

    #[rstest]
    #[case(65536.0 / 64.0)]
    #[case(f32::INFINITY)]
    fn a_sound_speed_above_the_field_is_rejected(#[case] m_s: f32) {
        assert_eq!(
            ConfigPatternPayload::foci(PatternBank::B0, DIVIDER, 2, 1, m_s, 0).err(),
            Some(PayloadBuildError::SoundSpeedTooLarge {
                m_s,
                max: 65535.0 / 64.0,
            })
        );
    }
}
