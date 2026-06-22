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

    #[must_use]
    pub const fn is_newer_than(self, other: Self) -> bool {
        #[allow(clippy::cast_possible_wrap)]
        let signed_diff = self.0.wrapping_sub(other.0) as i8;
        signed_diff > 0
    }

    #[must_use]
    pub const fn distance_from(self, other: Self) -> u8 {
        self.0.wrapping_sub(other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_within_same_half() {
        assert!(Seq::new(5).is_newer_than(Seq::new(3)));
        assert!(!Seq::new(3).is_newer_than(Seq::new(5)));
    }

    #[test]
    fn not_newer_when_equal() {
        for v in 0u8..=255 {
            let s = Seq::new(v);
            assert!(
                !s.is_newer_than(s),
                "SEQ {v} should not be newer than itself"
            );
        }
    }

    #[test]
    fn newer_across_wraparound() {
        assert!(Seq::new(0x02).is_newer_than(Seq::new(0xFE)));
        assert!(!Seq::new(0xFE).is_newer_than(Seq::new(0x02)));
    }

    #[test]
    fn next_wraps() {
        assert_eq!(Seq::new(0).next(), Seq::new(1));
        assert_eq!(Seq::new(0xFF).next(), Seq::ZERO);
    }

    #[test]
    fn distance_round_trip() {
        for a in (0u8..=255).step_by(7) {
            for b in (0u8..=255).step_by(11) {
                let d = Seq::new(a).distance_from(Seq::new(b));
                assert_eq!(Seq::new(b.wrapping_add(d)), Seq::new(a));
            }
        }
    }

    #[test]
    fn distance_at_window_edge() {
        for delta in 1u8..=127 {
            let a = Seq::new(delta);
            let b = Seq::ZERO;
            assert!(a.is_newer_than(b), "delta {delta} should classify a > b");
            assert_eq!(a.distance_from(b), delta);
        }
    }
}
