use zerocopy::{FromBytes, Immutable, IntoBytes};

use super::{Intensity, Phase};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, FromBytes, IntoBytes, Immutable)]
pub struct Emission {
    pub phase: Phase,
    pub intensity: Intensity,
}

impl Emission {
    pub const NULL: Self = Self {
        phase: Phase::ZERO,
        intensity: Intensity::MIN,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_is_zero_phase_min_intensity() {
        assert_eq!(Emission::NULL.phase, Phase::ZERO);
        assert_eq!(Emission::NULL.intensity, Intensity::MIN);
        assert_eq!(Emission::NULL, Emission::default());
    }
}
