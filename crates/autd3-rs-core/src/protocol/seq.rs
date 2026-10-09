#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Seq(u8);

impl Seq {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_wraps() {
        assert_eq!(Seq::new(0).next(), Seq::new(1));
        assert_eq!(Seq::new(0xFF).next(), Seq::ZERO);
    }
}
