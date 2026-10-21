#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use thiserror::Error;

pub const PULSE_WIDTH_PERIOD: u16 =
    (crate::params::FPGA_CLK_FREQ_HZ / crate::params::ULTRASOUND_FREQ_HZ) as u16;

#[derive(Clone, Copy, Debug, PartialEq, Error)]
#[non_exhaustive]
pub enum PulseWidthError {
    #[error("pulse width ({0}) is out of range [0, {n})", n = PULSE_WIDTH_PERIOD)]
    PulseWidthOutOfRange(u16),
    #[error("duty ratio ({0}) is out of range [0, 1)")]
    DutyRatioOutOfRange(f32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PulseWidth(u16);

impl PulseWidth {
    pub const ZERO: Self = Self(0);

    pub const fn new(pulse_width: u16) -> Result<Self, PulseWidthError> {
        if pulse_width >= PULSE_WIDTH_PERIOD {
            return Err(PulseWidthError::PulseWidthOutOfRange(pulse_width));
        }
        Ok(Self(pulse_width))
    }

    pub fn from_duty(duty: f32) -> Result<Self, PulseWidthError> {
        if !(0.0..1.0).contains(&duty) {
            return Err(PulseWidthError::DutyRatioOutOfRange(duty));
        }
        Self::new((duty * f32::from(PULSE_WIDTH_PERIOD)).round() as u16)
    }

    #[must_use]
    pub const fn pulse_width(self) -> u16 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_in_range() {
        assert_eq!(PulseWidth::new(0).map(PulseWidth::pulse_width), Ok(0));
        assert_eq!(PulseWidth::new(256).map(PulseWidth::pulse_width), Ok(256));
        assert_eq!(PulseWidth::new(511).map(PulseWidth::pulse_width), Ok(511));
        assert_eq!(
            PulseWidth::new(512),
            Err(PulseWidthError::PulseWidthOutOfRange(512))
        );
        assert_eq!(PulseWidth::default().pulse_width(), 0);
    }

    #[test]
    fn from_duty_rounds_and_validates() {
        assert_eq!(
            PulseWidth::from_duty(0.0).map(PulseWidth::pulse_width),
            Ok(0)
        );
        assert_eq!(
            PulseWidth::from_duty(0.5).map(PulseWidth::pulse_width),
            Ok(256)
        );
        assert_eq!(
            PulseWidth::from_duty(511.0 / 512.0).map(PulseWidth::pulse_width),
            Ok(511)
        );
        assert_eq!(
            PulseWidth::from_duty(-0.5),
            Err(PulseWidthError::DutyRatioOutOfRange(-0.5))
        );
        assert_eq!(
            PulseWidth::from_duty(1.0),
            Err(PulseWidthError::DutyRatioOutOfRange(1.0))
        );
    }
}
