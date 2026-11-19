pub struct Hz;

#[allow(non_camel_case_types)]
pub struct kHz;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Freq {
    pub(crate) freq: f32,
}

impl core::fmt::Debug for Freq {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} Hz", self.freq)
    }
}

impl Freq {
    #[must_use]
    pub const fn from_hz(freq: f32) -> Self {
        Self { freq }
    }

    #[inline]
    #[must_use]
    pub const fn hz(&self) -> f32 {
        self.freq
    }
}

impl core::ops::Add<Freq> for Freq {
    type Output = Freq;
    fn add(self, rhs: Freq) -> Self::Output {
        Freq {
            freq: self.freq + rhs.freq,
        }
    }
}

impl core::ops::Sub<Freq> for Freq {
    type Output = Freq;
    fn sub(self, rhs: Freq) -> Self::Output {
        Freq {
            freq: self.freq - rhs.freq,
        }
    }
}

impl core::ops::Mul<f32> for Freq {
    type Output = Freq;
    fn mul(self, rhs: f32) -> Self::Output {
        Freq {
            freq: self.freq * rhs,
        }
    }
}

impl core::ops::Div<f32> for Freq {
    type Output = Freq;
    fn div(self, rhs: f32) -> Self::Output {
        Freq {
            freq: self.freq / rhs,
        }
    }
}

impl core::ops::Mul<Hz> for u32 {
    type Output = Freq;
    fn mul(self, _rhs: Hz) -> Self::Output {
        Self::Output { freq: self as f32 }
    }
}

impl core::ops::Mul<kHz> for u32 {
    type Output = Freq;
    fn mul(self, _rhs: kHz) -> Self::Output {
        Self::Output {
            freq: self as f32 * 1e3,
        }
    }
}

impl core::ops::Mul<Hz> for f32 {
    type Output = Freq;
    fn mul(self, _rhs: Hz) -> Self::Output {
        Self::Output { freq: self }
    }
}

impl core::ops::Mul<kHz> for f32 {
    type Output = Freq;
    fn mul(self, _rhs: kHz) -> Self::Output {
        Self::Output { freq: self * 1e3 }
    }
}

impl core::ops::Mul<Freq> for f32 {
    type Output = Freq;
    fn mul(self, rhs: Freq) -> Self::Output {
        Self::Output {
            freq: self * rhs.freq,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops() {
        assert_eq!(200 * Hz, 100 * Hz + 100 * Hz);
        assert_eq!(0 * Hz, 100 * Hz - 100 * Hz);
        assert_eq!(200 * Hz, 100 * Hz * 2.);
        assert_eq!(200 * Hz, 2. * (100 * Hz));
        assert_eq!(50 * Hz, 100 * Hz / 2.);
    }

    #[test]
    fn ctor() {
        assert_eq!(Freq { freq: 200.0 }, 200 * Hz);
        assert_eq!(Freq { freq: 2000.0 }, 2 * kHz);
        assert_eq!(Freq { freq: 200.0 }, 200.0 * Hz);
        assert_eq!(Freq { freq: 2000.0 }, 2.0 * kHz);
        assert_eq!(200 * Hz, 200.0 * Hz);
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", 100 * Hz), "100 Hz");
        assert_eq!(format!("{:?}", 100 * kHz), "100000 Hz");
    }
}
