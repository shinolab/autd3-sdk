use core::num::NonZeroU16;
use core::time::Duration;

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use super::{PayloadBuildError, try_read_exact};
use crate::Error;
use crate::fpga_params::{SilencerFlags, ULTRASOUND_FREQ_HZ};

pub const SILENCER_DEFAULT_UPDATE_RATE: u16 = 256;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY: u16 = 10;
pub const SILENCER_DEFAULT_COMPLETION_STEPS_PHASE: u16 = 40;

fn completion_steps(time: Duration) -> Result<NonZeroU16, PayloadBuildError> {
    const NANOS_PER_SEC: u128 = 1_000_000_000;
    let scaled = time.as_nanos() * u128::from(ULTRASOUND_FREQ_HZ);
    if !scaled.is_multiple_of(NANOS_PER_SEC) {
        return Err(PayloadBuildError::SilencerCompletionTimeNotMultiple(time));
    }
    u16::try_from(scaled / NANOS_PER_SEC)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or(PayloadBuildError::SilencerCompletionTimeOutOfRange(time))
}

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SilencerPayload {
    pub flag: u8,
    pub reserved: u8,
    pub update_rate_intensity: U16,
    pub update_rate_phase: U16,
    pub completion_steps_intensity: U16,
    pub completion_steps_phase: U16,
}

impl SilencerPayload {
    #[must_use]
    pub const fn fixed_completion_steps(
        intensity: NonZeroU16,
        phase: NonZeroU16,
        strict_mode: bool,
    ) -> Self {
        let flags = if strict_mode {
            SilencerFlags::STRICT_MODE
        } else {
            SilencerFlags::empty()
        };
        Self {
            flag: flags.bits(),
            reserved: 0,
            update_rate_intensity: U16::new(SILENCER_DEFAULT_UPDATE_RATE),
            update_rate_phase: U16::new(SILENCER_DEFAULT_UPDATE_RATE),
            completion_steps_intensity: U16::new(intensity.get()),
            completion_steps_phase: U16::new(phase.get()),
        }
    }

    pub fn fixed_completion_time(
        intensity: Duration,
        phase: Duration,
        strict_mode: bool,
    ) -> Result<Self, PayloadBuildError> {
        Ok(Self::fixed_completion_steps(
            completion_steps(intensity)?,
            completion_steps(phase)?,
            strict_mode,
        ))
    }

    #[must_use]
    pub const fn fixed_update_rate(intensity: NonZeroU16, phase: NonZeroU16) -> Self {
        Self {
            flag: SilencerFlags::FIXED_UPDATE_RATE_MODE.bits(),
            reserved: 0,
            update_rate_intensity: U16::new(intensity.get()),
            update_rate_phase: U16::new(phase.get()),
            completion_steps_intensity: U16::new(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY),
            completion_steps_phase: U16::new(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE),
        }
    }

    #[must_use]
    pub const fn flags(&self) -> SilencerFlags {
        SilencerFlags::from_bits_retain(self.flag)
    }

    #[must_use]
    pub fn update_rate(&self) -> Option<(NonZeroU16, NonZeroU16)> {
        NonZeroU16::new(self.update_rate_intensity.get())
            .zip(NonZeroU16::new(self.update_rate_phase.get()))
    }

    #[must_use]
    pub fn completion_steps(&self) -> Option<(NonZeroU16, NonZeroU16)> {
        NonZeroU16::new(self.completion_steps_intensity.get())
            .zip(NonZeroU16::new(self.completion_steps_phase.get()))
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        let active = if p.flags().contains(SilencerFlags::FIXED_UPDATE_RATE_MODE) {
            p.update_rate()
        } else {
            p.completion_steps()
        };
        active.ok_or(Error::InvalidPayload)?;
        Ok(p)
    }
}

const _: () = assert!(core::mem::offset_of!(SilencerPayload, flag) == 0);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_intensity) == 2);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, update_rate_phase) == 4);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_intensity) == 6);
const _: () = assert!(core::mem::offset_of!(SilencerPayload, completion_steps_phase) == 8);
const _: () = assert!(core::mem::size_of::<SilencerPayload>() == 10);

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    const INTENSITY: NonZeroU16 = NonZeroU16::new(3).unwrap();
    const PHASE: NonZeroU16 = NonZeroU16::new(7).unwrap();

    #[rstest]
    #[case(true, SilencerFlags::STRICT_MODE)]
    #[case(false, SilencerFlags::empty())]
    fn fixed_completion_steps_passes_parse(
        #[case] strict_mode: bool,
        #[case] flags: SilencerFlags,
    ) {
        let built = SilencerPayload::fixed_completion_steps(INTENSITY, PHASE, strict_mode);
        let parsed = SilencerPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.flags(), flags);
        assert_eq!(parsed.completion_steps(), Some((INTENSITY, PHASE)));
        assert_eq!(
            parsed.update_rate().map(|(i, p)| (i.get(), p.get())),
            Some((SILENCER_DEFAULT_UPDATE_RATE, SILENCER_DEFAULT_UPDATE_RATE))
        );
    }

    #[test]
    fn fixed_update_rate_passes_parse() {
        let built = SilencerPayload::fixed_update_rate(INTENSITY, PHASE);
        let parsed = SilencerPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.flags(), SilencerFlags::FIXED_UPDATE_RATE_MODE);
        assert_eq!(parsed.update_rate(), Some((INTENSITY, PHASE)));
        assert_eq!(
            parsed.completion_steps().map(|(i, p)| (i.get(), p.get())),
            Some((
                SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
                SILENCER_DEFAULT_COMPLETION_STEPS_PHASE
            ))
        );
    }

    const PERIOD: Duration = Duration::from_micros(25);

    #[rstest]
    #[case(PERIOD, 1)]
    #[case(PERIOD * 10, 10)]
    #[case(PERIOD * 65535, u16::MAX)]
    fn a_completion_time_is_carried_in_ultrasound_periods(
        #[case] time: Duration,
        #[case] steps: u16,
    ) {
        let built = SilencerPayload::fixed_completion_time(time, PERIOD * 2, true).unwrap();
        assert_eq!(built.completion_steps_intensity.get(), steps);
        assert_eq!(built.completion_steps_phase.get(), 2);
        assert!(SilencerPayload::parse(built.as_bytes()).is_ok());
    }

    #[rstest]
    #[case(Duration::from_micros(26))]
    #[case(Duration::from_nanos(1))]
    fn a_completion_time_off_the_ultrasound_period_is_rejected(#[case] time: Duration) {
        assert_eq!(
            SilencerPayload::fixed_completion_time(time, PERIOD, true).err(),
            Some(PayloadBuildError::SilencerCompletionTimeNotMultiple(time))
        );
        assert_eq!(
            SilencerPayload::fixed_completion_time(PERIOD, time, true).err(),
            Some(PayloadBuildError::SilencerCompletionTimeNotMultiple(time))
        );
    }

    #[rstest]
    #[case(Duration::ZERO)]
    #[case(PERIOD * 65536)]
    fn a_completion_time_outside_the_field_is_rejected(#[case] time: Duration) {
        assert_eq!(
            SilencerPayload::fixed_completion_time(time, PERIOD, true).err(),
            Some(PayloadBuildError::SilencerCompletionTimeOutOfRange(time))
        );
        assert_eq!(
            SilencerPayload::fixed_completion_time(PERIOD, time, true).err(),
            Some(PayloadBuildError::SilencerCompletionTimeOutOfRange(time))
        );
    }
}
