use zerocopy::{FromBytes, Immutable, IntoBytes};

#[repr(C)]
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, FromBytes, IntoBytes, Immutable,
)]
pub struct Intensity(pub u8);

impl core::fmt::Debug for Intensity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "0x{:02X}", self.0)
    }
}

impl Intensity {
    pub const MAX: Intensity = Intensity(0xFF);
    pub const MIN: Intensity = Intensity(0x00);
}

impl core::ops::Div<u8> for Intensity {
    type Output = Self;
    fn div(self, rhs: u8) -> Self::Output {
        Self(self.0 / rhs)
    }
}

impl core::ops::Mul<u8> for Intensity {
    type Output = Intensity;
    fn mul(self, rhs: u8) -> Self::Output {
        Intensity(self.0.saturating_mul(rhs))
    }
}

impl core::ops::Mul<Intensity> for u8 {
    type Output = Intensity;
    fn mul(self, rhs: Intensity) -> Self::Output {
        Intensity(self.saturating_mul(rhs.0))
    }
}

impl core::ops::Add for Intensity {
    type Output = Self;
    fn add(self, rhs: Intensity) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl core::ops::AddAssign for Intensity {
    fn add_assign(&mut self, rhs: Intensity) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}

impl core::ops::Sub for Intensity {
    type Output = Self;
    fn sub(self, rhs: Intensity) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl core::ops::SubAssign for Intensity {
    fn sub_assign(&mut self, rhs: Intensity) {
        self.0 = self.0.saturating_sub(rhs.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn div() {
        for (expected, target, d) in [
            (Intensity(0x01), Intensity(0x01), 1),
            (Intensity(0x00), Intensity(0x01), 2),
            (Intensity(0x7F), Intensity(0xFF), 2),
        ] {
            assert_eq!(expected, target / d);
        }
    }

    #[test]
    fn mul_saturates() {
        for (expected, target, m) in [
            (Intensity(0x01), Intensity(0x01), 1),
            (Intensity(0x02), Intensity(0x01), 2),
            (Intensity(0xFE), Intensity(0x7F), 2),
            (Intensity(0xFF), Intensity(0x7F), 3),
        ] {
            assert_eq!(expected, target * m);
            assert_eq!(expected, m * target);
        }
    }

    #[test]
    fn add_saturates() {
        for (expected, lhs, rhs) in [
            (Intensity(0x02), Intensity(0x01), Intensity(0x01)),
            (Intensity(0xFE), Intensity(0x7F), Intensity(0x7F)),
            (Intensity(0xFF), Intensity(0x7F), Intensity(0xFF)),
        ] {
            assert_eq!(expected, lhs + rhs);
            let mut a = lhs;
            a += rhs;
            assert_eq!(expected, a);
        }
    }

    #[test]
    fn sub_saturates() {
        for (expected, lhs, rhs) in [
            (Intensity(0x00), Intensity(0x01), Intensity(0x01)),
            (Intensity(0x01), Intensity(0x02), Intensity(0x01)),
            (Intensity(0x00), Intensity(0x7F), Intensity(0xFF)),
        ] {
            assert_eq!(expected, lhs - rhs);
            let mut a = lhs;
            a -= rhs;
            assert_eq!(expected, a);
        }
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", Intensity(0x00)), "0x00");
        assert_eq!(format!("{:?}", Intensity(0xFF)), "0xFF");
    }
}
