#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use thiserror::Error;

pub const PULSE_WIDTH_PERIOD: u16 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Error)]
pub enum PulseWidthError {
    #[error("pulse width ({0}) is out of range [0, {n})", n = PULSE_WIDTH_PERIOD)]
    PulseWidthOutOfRange(u16),
    #[error("duty ratio ({0}) is out of range [0, 1)")]
    DutyRatioOutOfRange(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
enum PulseWidthInner {
    Duty(f32),
    Raw(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct PulseWidth {
    inner: PulseWidthInner,
}

impl PulseWidth {
    #[must_use]
    pub const fn new(pulse_width: u16) -> Self {
        Self {
            inner: PulseWidthInner::Raw(pulse_width),
        }
    }

    #[must_use]
    pub const fn from_duty(duty: f32) -> Self {
        Self {
            inner: PulseWidthInner::Duty(duty),
        }
    }

    pub fn pulse_width(self) -> Result<u16, PulseWidthError> {
        let pulse_width = match self.inner {
            PulseWidthInner::Duty(duty) => {
                if !(0.0..1.0).contains(&duty) {
                    return Err(PulseWidthError::DutyRatioOutOfRange(duty));
                }
                (duty * f32::from(PULSE_WIDTH_PERIOD)).round() as u16
            }
            PulseWidthInner::Raw(raw) => raw,
        };
        if pulse_width >= PULSE_WIDTH_PERIOD {
            return Err(PulseWidthError::PulseWidthOutOfRange(pulse_width));
        }
        Ok(pulse_width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_in_range() {
        assert_eq!(PulseWidth::new(0).pulse_width(), Ok(0));
        assert_eq!(PulseWidth::new(256).pulse_width(), Ok(256));
        assert_eq!(PulseWidth::new(511).pulse_width(), Ok(511));
        assert_eq!(
            PulseWidth::new(512).pulse_width(),
            Err(PulseWidthError::PulseWidthOutOfRange(512))
        );
    }

    #[test]
    fn from_duty_rounds_and_validates() {
        assert_eq!(PulseWidth::from_duty(0.0).pulse_width(), Ok(0));
        assert_eq!(PulseWidth::from_duty(0.5).pulse_width(), Ok(256));
        assert_eq!(PulseWidth::from_duty(511.0 / 512.0).pulse_width(), Ok(511));
        assert_eq!(
            PulseWidth::from_duty(-0.5).pulse_width(),
            Err(PulseWidthError::DutyRatioOutOfRange(-0.5))
        );
        assert_eq!(
            PulseWidth::from_duty(1.0).pulse_width(),
            Err(PulseWidthError::DutyRatioOutOfRange(1.0))
        );
    }
}
