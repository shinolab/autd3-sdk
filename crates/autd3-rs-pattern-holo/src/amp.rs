pub const ABSOLUTE_THRESHOLD_OF_HEARING: f32 = 20e-6;

#[allow(non_camel_case_types)]
pub struct dB;

pub struct Pa;

#[allow(non_camel_case_types)]
pub struct kPa;

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Amplitude {
    value: f32,
}

impl Amplitude {
    #[must_use]
    pub const fn pascal(&self) -> f32 {
        self.value
    }

    #[must_use]
    pub fn spl(&self) -> f32 {
        20.0 * f32::log10(self.value / ABSOLUTE_THRESHOLD_OF_HEARING)
    }
}

impl core::ops::Mul<dB> for f32 {
    type Output = Amplitude;
    fn mul(self, _rhs: dB) -> Self::Output {
        Self::Output {
            value: ABSOLUTE_THRESHOLD_OF_HEARING * f32::powf(10.0, self / 20.0),
        }
    }
}

impl core::ops::Mul<Pa> for f32 {
    type Output = Amplitude;
    fn mul(self, _rhs: Pa) -> Self::Output {
        Self::Output { value: self }
    }
}

impl core::ops::Mul<kPa> for f32 {
    type Output = Amplitude;
    fn mul(self, _rhs: kPa) -> Self::Output {
        Self::Output { value: self * 1e3 }
    }
}

impl core::ops::Mul<f32> for Amplitude {
    type Output = Amplitude;
    fn mul(self, rhs: f32) -> Self::Output {
        Self::Output {
            value: self.value * rhs,
        }
    }
}

impl core::ops::Mul<Amplitude> for f32 {
    type Output = Amplitude;
    fn mul(self, rhs: Amplitude) -> Self::Output {
        Self::Output {
            value: self * rhs.value,
        }
    }
}

impl core::ops::Div<f32> for Amplitude {
    type Output = Amplitude;
    fn div(self, rhs: f32) -> Self::Output {
        Self::Output {
            value: self.value / rhs,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db() {
        let amp = 121.5 * dB;
        approx::assert_abs_diff_eq!(amp.spl(), 121.5, epsilon = 1e-3);
        approx::assert_abs_diff_eq!(amp.pascal(), 23.77, epsilon = 1e-3);
    }

    #[test]
    fn pascal() {
        let amp = 23.77 * Pa;
        approx::assert_abs_diff_eq!(amp.pascal(), 23.77, epsilon = 1e-3);
        approx::assert_abs_diff_eq!(amp.spl(), 121.5, epsilon = 1e-3);
        approx::assert_abs_diff_eq!((2. * amp).pascal(), 2. * 23.77, epsilon = 1e-3);
        approx::assert_abs_diff_eq!((amp * 2.).pascal(), 2. * 23.77, epsilon = 1e-3);
        approx::assert_abs_diff_eq!((amp / 2.).pascal(), 23.77 / 2., epsilon = 1e-3);
    }

    #[test]
    fn kilo_pascal() {
        let amp = 23.77e-3 * kPa;
        approx::assert_abs_diff_eq!(amp.pascal(), 23.77, epsilon = 1e-3);
        approx::assert_abs_diff_eq!(amp.spl(), 121.5, epsilon = 1e-3);
    }
}
