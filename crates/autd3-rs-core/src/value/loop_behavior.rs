use core::num::NonZeroU16;

const REP_INFINITE: u16 = 0xFFFF;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LoopBehavior {
    #[default]
    Infinite,
    Finite(NonZeroU16),
}

impl LoopBehavior {
    pub const ONCE: Self = Self::Finite(NonZeroU16::MIN);

    #[must_use]
    pub const fn rep(self) -> u16 {
        match self {
            LoopBehavior::Infinite => REP_INFINITE,
            LoopBehavior::Finite(n) => n.get() - 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rep_conversion() {
        assert_eq!(LoopBehavior::Infinite.rep(), 0xFFFF);
        assert_eq!(LoopBehavior::ONCE.rep(), 0);
        assert_eq!(LoopBehavior::Finite(NonZeroU16::new(10).unwrap()).rep(), 9);
        assert_eq!(
            LoopBehavior::Finite(NonZeroU16::new(0xFFFF).unwrap()).rep(),
            0xFFFE
        );
    }

    #[test]
    fn default_is_infinite() {
        assert_eq!(LoopBehavior::default(), LoopBehavior::Infinite);
    }
}
