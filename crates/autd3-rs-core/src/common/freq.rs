pub struct Hz;

#[allow(non_camel_case_types)]
pub struct kHz;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub struct Freq<T: Copy> {
    pub(crate) freq: T,
}

impl<T: Copy> core::fmt::Debug for Freq<T>
where
    T: core::fmt::Display,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} Hz", self.freq)
    }
}

impl<T: Copy> Freq<T> {
    #[inline]
    #[must_use]
    pub const fn hz(&self) -> T {
        self.freq
    }
}

impl<T> core::ops::Add<Freq<T>> for Freq<T>
where
    T: core::ops::Add<Output = T> + Copy,
{
    type Output = Freq<T>;
    fn add(self, rhs: Freq<T>) -> Self::Output {
        Freq {
            freq: self.freq + rhs.freq,
        }
    }
}

impl<T> core::ops::Sub<Freq<T>> for Freq<T>
where
    T: core::ops::Sub<Output = T> + Copy,
{
    type Output = Freq<T>;
    fn sub(self, rhs: Freq<T>) -> Self::Output {
        Freq {
            freq: self.freq - rhs.freq,
        }
    }
}

impl<T, U> core::ops::Mul<U> for Freq<T>
where
    T: core::ops::Mul<U, Output = T> + Copy,
{
    type Output = Freq<T>;
    fn mul(self, rhs: U) -> Self::Output {
        Freq {
            freq: self.freq * rhs,
        }
    }
}

impl<T, U> core::ops::Div<U> for Freq<T>
where
    T: core::ops::Div<U, Output = T> + Copy,
{
    type Output = Freq<T>;
    fn div(self, rhs: U) -> Self::Output {
        Freq {
            freq: self.freq / rhs,
        }
    }
}

impl core::ops::Mul<Hz> for u32 {
    type Output = Freq<u32>;
    fn mul(self, _rhs: Hz) -> Self::Output {
        Self::Output { freq: self }
    }
}

impl core::ops::Mul<kHz> for u32 {
    type Output = Freq<u32>;
    fn mul(self, _rhs: kHz) -> Self::Output {
        Self::Output { freq: self * 1000 }
    }
}

impl core::ops::Mul<Freq<u32>> for u32 {
    type Output = Freq<u32>;
    fn mul(self, rhs: Freq<u32>) -> Self::Output {
        Self::Output {
            freq: self * rhs.freq,
        }
    }
}

impl core::ops::Mul<Hz> for f32 {
    type Output = Freq<f32>;
    fn mul(self, _rhs: Hz) -> Self::Output {
        Self::Output { freq: self }
    }
}

impl core::ops::Mul<kHz> for f32 {
    type Output = Freq<f32>;
    fn mul(self, _rhs: kHz) -> Self::Output {
        Self::Output { freq: self * 1e3 }
    }
}

impl core::ops::Mul<Freq<f32>> for f32 {
    type Output = Freq<f32>;
    fn mul(self, rhs: Freq<f32>) -> Self::Output {
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
        assert_eq!(200 * Hz, 100 * Hz * 2);
        assert_eq!(50 * Hz, 100 * Hz / 2);
    }

    #[test]
    fn ctor() {
        assert_eq!(Freq { freq: 200 }, 200 * Hz);
        assert_eq!(Freq { freq: 2000 }, 2 * kHz);
        assert_eq!(Freq { freq: 200.0 }, 200.0 * Hz);
        assert_eq!(Freq { freq: 2000.0 }, 2.0 * kHz);
    }

    #[test]
    fn dbg() {
        assert_eq!(format!("{:?}", 100 * Hz), "100 Hz");
        assert_eq!(format!("{:?}", 100 * kHz), "100000 Hz");
    }
}
