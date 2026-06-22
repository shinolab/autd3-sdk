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
    pub const fn radian(&self) -> f32 {
        self.0 as f32 / 256.0 * 2.0 * PI
    }
}

impl From<Angle> for Phase {
    fn from(v: Angle) -> Self {
        let p = (v.radian() / (2.0 * PI) * 256.0).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Self(((p as i32) & 0xFF) as u8)
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

    #[test]
    fn add_wraps() {
        for (expected, lhs, rhs) in [
            (Phase(0x02), Phase(0x01), Phase(0x01)),
            (Phase(0xFE), Phase(0x7F), Phase(0x7F)),
            (Phase(0x7E), Phase(0x7F), Phase(0xFF)),
        ] {
            assert_eq!(expected, lhs + rhs);
            let mut a = lhs;
            a += rhs;
            assert_eq!(expected, a);
        }
    }

    #[test]
    fn sub_wraps() {
        for (expected, lhs, rhs) in [
            (Phase::ZERO, Phase(0x01), Phase(0x01)),
            (Phase(0x01), Phase(0x02), Phase(0x01)),
            (Phase(0x80), Phase(0x7F), Phase(0xFF)),
        ] {
            assert_eq!(expected, lhs - rhs);
            let mut a = lhs;
            a -= rhs;
            assert_eq!(expected, a);
        }
    }

    #[test]
    fn mul_wraps() {
        for (expected, lhs, rhs) in [
            (Phase(0x02), Phase(0x01), 2),
            (Phase(0xFE), Phase(0x7F), 2),
            (Phase::ZERO, Phase(0x80), 2),
        ] {
            assert_eq!(expected, lhs * rhs);
            assert_eq!(expected, rhs * lhs);
        }
    }

    #[test]
    fn div() {
        for (expected, lhs, rhs) in [(Phase(0x01), Phase(0x02), 2), (Phase(0x7F), Phase(0xFE), 2)] {
            assert_eq!(expected, lhs / rhs);
        }
    }

    #[test]
    fn radian() {
        for (expect, value) in [
            (0.0, 0u8),
            (2.0 * PI / 256.0 * 128.0, 128),
            (2.0 * PI / 256.0 * 255.0, 255),
        ] {
            approx::assert_abs_diff_eq!(expect, Phase(value).radian());
        }
    }

    #[test]
    fn from_complex() {
        for (expect, value) in [
            (Phase(0x00), Complex::new(1.0, 0.0)),
            (Phase(0x40), Complex::new(0.0, 1.0)),
            (Phase(0x80), Complex::new(-1.0, 0.0)),
            (Phase(0xC0), Complex::new(0.0, -1.0)),
        ] {
            assert_eq!(expect, Phase::from(value));
        }
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", Phase::ZERO), "0x00");
        assert_eq!(format!("{:?}", Phase(0xFF)), "0xFF");
    }
}
