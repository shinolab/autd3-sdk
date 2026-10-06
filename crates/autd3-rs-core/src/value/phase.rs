use core::f32::consts::PI;

use nalgebra::Complex;
use zerocopy::{FromBytes, Immutable, IntoBytes};

use crate::common::{Angle, units::rad};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Default, FromBytes, IntoBytes, Immutable)]
pub struct Phase(pub u8);

impl core::fmt::Debug for Phase {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{:02X}", self.0)
    }
}

impl Phase {
    pub const ZERO: Self = Self(0);
    pub const PI: Self = Self(0x80);

    #[must_use]
    pub const fn rad(&self) -> f32 {
        self.0 as f32 / 256.0 * 2.0 * PI
    }
}

const RAD_PER_LSB: f32 = 2.0 * PI / 256.0;
const LSB_LIMIT: f32 = 2_147_483_648.0;
const ROUND_TO_MULTIPLE_OF_512: f32 = 6_442_450_944.0;
const ROUND_TO_INTEGER: f32 = 12_582_912.0;

impl From<Angle> for Phase {
    #[inline]
    fn from(v: Angle) -> Self {
        #[allow(clippy::manual_clamp)]
        let lsb = (v.rad() / RAD_PER_LSB).max(-LSB_LIMIT).min(LSB_LIMIT);
        let turns = (lsb + ROUND_TO_MULTIPLE_OF_512) - ROUND_TO_MULTIPLE_OF_512;
        let within_turns = lsb - turns;
        let rounded = within_turns + ROUND_TO_INTEGER;
        #[allow(clippy::float_cmp)]
        let away_from_zero = {
            let frac = within_turns - (rounded - ROUND_TO_INTEGER);
            i32::from(frac == 0.5 && lsb > 0.0) - i32::from(frac == -0.5 && lsb < 0.0)
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self(rounded.to_bits().wrapping_add(away_from_zero as u32) as u8)
    }
}

impl From<Complex<f32>> for Phase {
    fn from(v: Complex<f32>) -> Self {
        Self::from(v.arg() * rad)
    }
}

impl core::ops::Add<Phase> for Phase {
    type Output = Phase;
    fn add(self, rhs: Phase) -> Self::Output {
        Phase(self.0.wrapping_add(rhs.0))
    }
}

impl core::ops::AddAssign for Phase {
    fn add_assign(&mut self, rhs: Phase) {
        self.0 = self.0.wrapping_add(rhs.0);
    }
}

impl core::ops::Sub<Phase> for Phase {
    type Output = Phase;
    fn sub(self, rhs: Phase) -> Self::Output {
        Phase(self.0.wrapping_sub(rhs.0))
    }
}

impl core::ops::SubAssign for Phase {
    fn sub_assign(&mut self, rhs: Phase) {
        self.0 = self.0.wrapping_sub(rhs.0);
    }
}

impl core::ops::Mul<u8> for Phase {
    type Output = Phase;
    fn mul(self, rhs: u8) -> Self::Output {
        Phase(self.0.wrapping_mul(rhs))
    }
}

impl core::ops::Mul<Phase> for u8 {
    type Output = Phase;
    fn mul(self, rhs: Phase) -> Self::Output {
        Phase(self.wrapping_mul(rhs.0))
    }
}

impl core::ops::Div<u8> for Phase {
    type Output = Phase;
    fn div(self, rhs: u8) -> Self::Output {
        Phase(self.0.wrapping_div(rhs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(Phase(0x02), Phase(0x01), Phase(0x01))]
    #[case(Phase(0xFE), Phase(0x7F), Phase(0x7F))]
    #[case(Phase(0x7E), Phase(0x7F), Phase(0xFF))]
    fn add_wraps(#[case] expected: Phase, #[case] lhs: Phase, #[case] rhs: Phase) {
        assert_eq!(expected, lhs + rhs);
        let mut a = lhs;
        a += rhs;
        assert_eq!(expected, a);
    }

    #[rstest]
    #[case(Phase::ZERO, Phase(0x01), Phase(0x01))]
    #[case(Phase(0x01), Phase(0x02), Phase(0x01))]
    #[case(Phase(0x80), Phase(0x7F), Phase(0xFF))]
    fn sub_wraps(#[case] expected: Phase, #[case] lhs: Phase, #[case] rhs: Phase) {
        assert_eq!(expected, lhs - rhs);
        let mut a = lhs;
        a -= rhs;
        assert_eq!(expected, a);
    }

    #[rstest]
    #[case(Phase(0x02), Phase(0x01), 2)]
    #[case(Phase(0xFE), Phase(0x7F), 2)]
    #[case(Phase::ZERO, Phase(0x80), 2)]
    fn mul_wraps(#[case] expected: Phase, #[case] lhs: Phase, #[case] rhs: u8) {
        assert_eq!(expected, lhs * rhs);
        assert_eq!(expected, rhs * lhs);
    }

    #[rstest]
    #[case(Phase(0x01), Phase(0x02), 2)]
    #[case(Phase(0x7F), Phase(0xFE), 2)]
    fn div(#[case] expected: Phase, #[case] lhs: Phase, #[case] rhs: u8) {
        assert_eq!(expected, lhs / rhs);
    }

    #[rstest]
    #[case(0.0, 0)]
    #[case(2.0 * PI / 256.0 * 128.0, 128)]
    #[case(2.0 * PI / 256.0 * 255.0, 255)]
    fn rad(#[case] expect: f32, #[case] value: u8) {
        approx::assert_abs_diff_eq!(expect, Phase(value).rad());
    }

    fn quantized_exactly(v: Angle) -> Phase {
        let lsb = f64::from(v.rad() / RAD_PER_LSB);
        if !lsb.is_finite() || lsb.abs() >= f64::from(LSB_LIMIT) {
            return Phase::ZERO;
        }
        let truncated = lsb.trunc();
        let frac = lsb - truncated;
        let rounded = if frac >= 0.5 {
            truncated + 1.0
        } else if frac <= -0.5 {
            truncated - 1.0
        } else {
            truncated
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Phase(((rounded as i64) & 0xFF) as u8)
    }

    fn quantized_by_saturating_round(v: Angle) -> Phase {
        let p = (v.rad() / (2.0 * PI) * 256.0).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Phase(((p as i32) & 0xFF) as u8)
    }

    #[rstest]
    #[case(Phase(0x00), 0.0)]
    #[case(Phase(0x40), PI / 2.0)]
    #[case(Phase(0x80), PI)]
    #[case(Phase(0xC0), -PI / 2.0)]
    #[case(Phase(0x00), 2.0 * PI)]
    #[case(Phase(0x01), 2.0 * PI / 256.0)]
    #[case(Phase(0xFF), -2.0 * PI / 256.0)]
    fn from_angle(#[case] expect: Phase, #[case] value: f32) {
        assert_eq!(expect, Phase::from(Angle::from_rad(value)));
    }

    #[rstest]
    #[case(f32::NAN)]
    #[case(-f32::NAN)]
    #[case(f32::INFINITY)]
    #[case(f32::NEG_INFINITY)]
    #[case(f32::MAX)]
    #[case(f32::MIN)]
    #[case(f32::MIN_POSITIVE)]
    #[case(-f32::MIN_POSITIVE)]
    #[case(f32::from_bits(1))]
    #[case(f32::from_bits(0x8000_0001))]
    #[case(0.0)]
    #[case(-0.0)]
    #[case(RAD_PER_LSB / 2.0)]
    #[case(-RAD_PER_LSB / 2.0)]
    #[case(RAD_PER_LSB * 1.5)]
    #[case(-RAD_PER_LSB * 1.5)]
    #[case(RAD_PER_LSB * 2.5)]
    #[case(-RAD_PER_LSB * 2.5)]
    #[case(RAD_PER_LSB * 255.5)]
    #[case(RAD_PER_LSB * 256.5)]
    #[case(RAD_PER_LSB * 511.5)]
    #[case(RAD_PER_LSB * 512.5)]
    #[case(RAD_PER_LSB * LSB_LIMIT)]
    #[case(-RAD_PER_LSB * LSB_LIMIT)]
    fn from_angle_matches_exact_at_edges(#[case] value: f32) {
        let v = Angle::from_rad(value);
        assert_eq!(
            quantized_exactly(v),
            Phase::from(v),
            "{value:e} (0x{:08X})",
            value.to_bits()
        );
    }

    #[test]
    fn from_angle_wraps_beyond_the_saturating_boundary() {
        let boundary = RAD_PER_LSB * LSB_LIMIT;
        assert_eq!(Phase::ZERO, Phase::from(Angle::from_rad(boundary)));
        assert_eq!(
            Phase(0xFF),
            quantized_by_saturating_round(Angle::from_rad(boundary))
        );
        assert_eq!(Phase::ZERO, Phase::from(Angle::from_rad(f32::INFINITY)));
        assert_eq!(Phase::ZERO, Phase::from(Angle::from_rad(f32::NEG_INFINITY)));
        assert_eq!(Phase::ZERO, Phase::from(Angle::from_rad(f32::NAN)));
    }

    #[test]
    #[ignore = "sweeps all 2^32 f32 inputs"]
    fn from_angle_matches_exact_for_every_f32() {
        let mut bits = 0u32;
        loop {
            let value = f32::from_bits(bits);
            let v = Angle::from_rad(value);
            assert_eq!(
                quantized_exactly(v),
                Phase::from(v),
                "{value:e} (0x{bits:08X})"
            );
            if bits == u32::MAX {
                break;
            }
            bits += 1;
        }
    }

    #[test]
    #[ignore = "sweeps all 2^32 f32 inputs"]
    fn from_angle_matches_saturating_round_below_the_boundary() {
        let mut bits = 0u32;
        loop {
            let value = f32::from_bits(bits);
            if value.abs() < RAD_PER_LSB * LSB_LIMIT {
                let v = Angle::from_rad(value);
                assert_eq!(
                    quantized_by_saturating_round(v),
                    Phase::from(v),
                    "{value:e} (0x{bits:08X})"
                );
            }
            if bits == u32::MAX {
                break;
            }
            bits += 1;
        }
    }

    #[rstest]
    #[case(Phase(0x00), Complex::new(1.0, 0.0))]
    #[case(Phase(0x40), Complex::new(0.0, 1.0))]
    #[case(Phase(0x80), Complex::new(-1.0, 0.0))]
    #[case(Phase(0xC0), Complex::new(0.0, -1.0))]
    fn from_complex(#[case] expect: Phase, #[case] value: Complex<f32>) {
        assert_eq!(expect, Phase::from(value));
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", Phase::ZERO), "0x00");
        assert_eq!(format!("{:?}", Phase(0xFF)), "0xFF");
    }
}
