#[allow(non_camel_case_types)]
pub struct m;

#[allow(non_camel_case_types)]
pub struct mm;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Length {
    millimetres: f32,
}

impl core::fmt::Debug for Length {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} mm", self.millimetres)
    }
}

impl Length {
    #[must_use]
    pub const fn millimeters(millimetres: f32) -> Self {
        Self { millimetres }
    }

    #[must_use]
    pub const fn mm(self) -> f32 {
        self.millimetres
    }

    #[must_use]
    pub const fn m(self) -> f32 {
        self.millimetres / 1000.0
    }
}

impl core::ops::Mul<m> for f32 {
    type Output = Length;
    fn mul(self, _rhs: m) -> Self::Output {
        Length {
            millimetres: self * 1000.0,
        }
    }
}

impl core::ops::Mul<mm> for f32 {
    type Output = Length;
    fn mul(self, _rhs: mm) -> Self::Output {
        Length { millimetres: self }
    }
}

impl core::ops::Mul<m> for i32 {
    type Output = Length;
    fn mul(self, _rhs: m) -> Self::Output {
        Length {
            millimetres: self as f32 * 1000.0,
        }
    }
}

impl core::ops::Mul<mm> for i32 {
    type Output = Length;
    fn mul(self, _rhs: mm) -> Self::Output {
        Length {
            millimetres: self as f32,
        }
    }
}

impl core::ops::Mul<f32> for Length {
    type Output = Length;
    fn mul(self, rhs: f32) -> Self::Output {
        Length {
            millimetres: self.millimetres * rhs,
        }
    }
}

impl core::ops::Mul<Length> for f32 {
    type Output = Length;
    fn mul(self, rhs: Length) -> Self::Output {
        Length {
            millimetres: self * rhs.millimetres,
        }
    }
}

impl core::ops::Div<f32> for Length {
    type Output = Length;
    fn div(self, rhs: f32) -> Self::Output {
        Length {
            millimetres: self.millimetres / rhs,
        }
    }
}

impl core::ops::Add<Length> for Length {
    type Output = Length;
    fn add(self, rhs: Length) -> Self::Output {
        Length {
            millimetres: self.millimetres + rhs.millimetres,
        }
    }
}

impl core::ops::Sub<Length> for Length {
    type Output = Length;
    fn sub(self, rhs: Length) -> Self::Output {
        Length {
            millimetres: self.millimetres - rhs.millimetres,
        }
    }
}

impl core::ops::Neg for Length {
    type Output = Length;
    fn neg(self) -> Self::Output {
        Length {
            millimetres: -self.millimetres,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literals() {
        approx::assert_abs_diff_eq!((1.0 * m).mm(), 1000.0);
        approx::assert_abs_diff_eq!((10.0 * mm).mm(), 10.0);
        approx::assert_abs_diff_eq!((150 * mm).mm(), 150.0);
        approx::assert_abs_diff_eq!((2 * m).mm(), 2000.0);
        approx::assert_abs_diff_eq!((0.15 * m).m(), 0.15);
    }

    #[test]
    fn ops() {
        approx::assert_abs_diff_eq!((2.0 * (3.0 * mm)).mm(), 6.0);
        approx::assert_abs_diff_eq!(((3.0 * mm) * 2.0).mm(), 6.0);
        approx::assert_abs_diff_eq!(((6.0 * mm) / 2.0).mm(), 3.0);
        approx::assert_abs_diff_eq!((1.0 * mm + 2.0 * mm).mm(), 3.0);
        approx::assert_abs_diff_eq!((3.0 * mm - 1.0 * mm).mm(), 2.0);
        approx::assert_abs_diff_eq!((-(1.0 * mm)).mm(), -1.0);
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", 8.5 * mm), "8.5 mm");
    }
}
