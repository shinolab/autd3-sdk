use super::length::Length;

#[allow(non_camel_case_types)]
pub struct s;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Velocity {
    mm_per_s: f32,
}

impl core::fmt::Debug for Velocity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} mm/s", self.mm_per_s)
    }
}

impl Velocity {
    #[must_use]
    pub const fn from_mm_s(mm_per_s: f32) -> Self {
        Self { mm_per_s }
    }

    #[must_use]
    pub const fn from_m_s(m_per_s: f32) -> Self {
        Self {
            mm_per_s: m_per_s * 1000.0,
        }
    }

    #[must_use]
    pub const fn mm_per_s(self) -> f32 {
        self.mm_per_s
    }

    #[must_use]
    pub const fn m_s(self) -> f32 {
        self.mm_per_s / 1000.0
    }
}

impl core::ops::Div<s> for Length {
    type Output = Velocity;
    fn div(self, _rhs: s) -> Self::Output {
        Velocity {
            mm_per_s: self.mm(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::length::{m, mm};
    use super::*;

    #[test]
    fn from_length_per_s() {
        approx::assert_abs_diff_eq!((340.0 * m / s).mm_per_s(), 340_000.0);
        approx::assert_abs_diff_eq!((340 * m / s).mm_per_s(), 340_000.0);
        approx::assert_abs_diff_eq!((340_000.0 * mm / s).mm_per_s(), 340_000.0);
    }

    #[test]
    fn constructors() {
        approx::assert_abs_diff_eq!(Velocity::from_m_s(340.0).mm_per_s(), 340_000.0);
        approx::assert_abs_diff_eq!(Velocity::from_mm_s(340_000.0).mm_per_s(), 340_000.0);
        approx::assert_abs_diff_eq!(Velocity::from_mm_s(340_000.0).m_s(), 340.0);
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", 340.0 * m / s), "340000 mm/s");
    }
}
