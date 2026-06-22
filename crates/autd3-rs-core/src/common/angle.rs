#[allow(non_camel_case_types)]
pub struct deg;

#[allow(non_camel_case_types)]
pub struct rad;

#[repr(C)]
#[derive(Clone, Copy, PartialEq)]
pub struct Angle {
    radian: f32,
}

impl core::fmt::Debug for Angle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} rad", self.radian)
    }
}

impl Angle {
    pub const ZERO: Self = Self { radian: 0.0 };
    pub const PI: Self = Self {
        radian: core::f32::consts::PI,
    };

    #[must_use]
    pub const fn from_radian(radian: f32) -> Self {
        Self { radian }
    }

    #[must_use]
    pub const fn from_degree(degree: f32) -> Self {
        Self {
            radian: degree.to_radians(),
        }
    }

    #[must_use]
    pub const fn radian(self) -> f32 {
        self.radian
    }

    #[must_use]
    pub const fn degree(self) -> f32 {
        self.radian.to_degrees()
    }
}

impl core::ops::Mul<deg> for f32 {
    type Output = Angle;
    fn mul(self, _rhs: deg) -> Self::Output {
        Self::Output::from_degree(self)
    }
}

impl core::ops::Mul<rad> for f32 {
    type Output = Angle;
    fn mul(self, _rhs: rad) -> Self::Output {
        Self::Output::from_radian(self)
    }
}

impl core::ops::Neg for Angle {
    type Output = Angle;
    fn neg(self) -> Self::Output {
        Angle {
            radian: -self.radian,
        }
    }
}

impl core::ops::Add<Angle> for Angle {
    type Output = Angle;
    fn add(self, rhs: Angle) -> Self::Output {
        Angle {
            radian: self.radian + rhs.radian,
        }
    }
}

impl core::ops::Sub<Angle> for Angle {
    type Output = Angle;
    fn sub(self, rhs: Angle) -> Self::Output {
        Angle {
            radian: self.radian - rhs.radian,
        }
    }
}

impl core::ops::Mul<f32> for Angle {
    type Output = Angle;
    fn mul(self, rhs: f32) -> Self::Output {
        Angle {
            radian: self.radian * rhs,
        }
    }
}

impl core::ops::Div<f32> for Angle {
    type Output = Angle;
    fn div(self, rhs: f32) -> Self::Output {
        Angle {
            radian: self.radian / rhs,
        }
    }
}

impl core::ops::Mul<Angle> for f32 {
    type Output = Angle;
    fn mul(self, rhs: Angle) -> Self::Output {
        Angle {
            radian: rhs.radian * self,
        }
    }
}

impl core::ops::AddAssign for Angle {
    fn add_assign(&mut self, rhs: Angle) {
        self.radian += rhs.radian;
    }
}

impl core::ops::SubAssign for Angle {
    fn sub_assign(&mut self, rhs: Angle) {
        self.radian -= rhs.radian;
    }
}

impl core::ops::MulAssign<f32> for Angle {
    fn mul_assign(&mut self, rhs: f32) {
        self.radian *= rhs;
    }
}

impl core::ops::DivAssign<f32> for Angle {
    fn div_assign(&mut self, rhs: f32) {
        self.radian /= rhs;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", 1.0 * rad), "1 rad");
    }

    #[test]
    fn ops() {
        use approx::assert_abs_diff_eq;

        let mut a = 1.0 * rad;
        let b = 2.0 * rad;

        assert_abs_diff_eq!((-a).radian(), -1.0);
        assert_abs_diff_eq!((a + b).radian(), 3.0);
        assert_abs_diff_eq!((a - b).radian(), -1.0);
        assert_abs_diff_eq!((a * 2.0).radian(), 2.0);
        assert_abs_diff_eq!((a / 2.0).radian(), 0.5);
        assert_abs_diff_eq!((2.0 * a).radian(), 2.0);

        a += b;
        assert_abs_diff_eq!(a.radian(), 3.0);
        a -= b;
        assert_abs_diff_eq!(a.radian(), 1.0);
        a *= 2.0;
        assert_abs_diff_eq!(a.radian(), 2.0);
        a /= 2.0;
        assert_abs_diff_eq!(a.radian(), 1.0);
    }
}
