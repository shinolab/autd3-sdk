use zerocopy::{FromBytes, Immutable, IntoBytes};

use super::{Intensity, Phase};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, FromBytes, IntoBytes, Immutable)]
pub struct Emission {
    pub phase: Phase,
    pub intensity: Intensity,
}
