use autd3_rs_core::geometry::Device;
use zerocopy::{Immutable, IntoBytes};

use super::LegacyOperation;
use crate::legacy::error::LegacyError;
use crate::legacy::wire::{InfoType, Tag};

#[repr(C)]
#[derive(Clone, Copy, IntoBytes, Immutable)]
struct TagPair {
    tag: u8,
    value: u8,
}

fn write<T: IntoBytes + Immutable>(tx: &mut [u8], msg: &T) -> usize {
    let bytes = msg.as_bytes();
    tx[..bytes.len()].copy_from_slice(bytes);
    bytes.len()
}

macro_rules! tag_pair_op {
    ($name:ident, $tag:expr, $value:expr) => {
        impl LegacyOperation for $name {
            fn required_size(&self, _device: &Device) -> usize {
                size_of::<TagPair>()
            }

            fn pack(&mut self, _device: &Device, tx: &mut [u8]) -> Result<usize, LegacyError> {
                let value = $value(self);
                self.done = true;
                Ok(write(
                    tx,
                    &TagPair {
                        tag: $tag.as_u8(),
                        value,
                    },
                ))
            }

            fn is_done(&self) -> bool {
                self.done
            }
        }
    };
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Nop {
    done: bool,
}

impl Nop {
    #[must_use]
    pub const fn new() -> Self {
        Self { done: false }
    }
}

tag_pair_op!(Nop, Tag::Nop, |_: &Nop| 0u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clear {
    done: bool,
}

impl Clear {
    #[must_use]
    pub const fn new() -> Self {
        Self { done: false }
    }
}

tag_pair_op!(Clear, Tag::Clear, |_: &Clear| 0u8);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sync {
    done: bool,
}

impl Sync {
    #[must_use]
    pub const fn new() -> Self {
        Self { done: false }
    }
}

tag_pair_op!(Sync, Tag::Sync, |_: &Sync| 0u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmInfo {
    ty: InfoType,
    done: bool,
}

impl FirmInfo {
    #[must_use]
    pub const fn new(ty: InfoType) -> Self {
        Self { ty, done: false }
    }
}

tag_pair_op!(FirmInfo, Tag::FirmInfo, |s: &FirmInfo| s.ty.as_u8());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForceFan {
    value: bool,
    done: bool,
}

impl ForceFan {
    #[must_use]
    pub const fn new(value: bool) -> Self {
        Self { value, done: false }
    }
}

tag_pair_op!(ForceFan, Tag::ForceFan, |s: &ForceFan| u8::from(s.value));

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadsFpgaState {
    value: bool,
    done: bool,
}

impl ReadsFpgaState {
    #[must_use]
    pub const fn new(value: bool) -> Self {
        Self { value, done: false }
    }
}

tag_pair_op!(ReadsFpgaState, Tag::ReadsFpgaState, |s: &ReadsFpgaState| {
    u8::from(s.value)
});

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry};

    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default()])
    }

    fn packed<O: LegacyOperation>(mut op: O) -> [u8; 2] {
        let geo = geometry();
        let mut tx = [0xFFu8; 2];
        assert_eq!(op.required_size(&geo[0]), 2);
        assert!(!op.is_done());
        assert_eq!(op.pack(&geo[0], &mut tx).unwrap(), 2);
        assert!(op.is_done());
        tx
    }

    #[test]
    fn tag_only_ops_pad_the_second_byte_with_zero() {
        assert_eq!(packed(Nop::new()), [Tag::Nop.as_u8(), 0x00]);
        assert_eq!(packed(Clear::new()), [Tag::Clear.as_u8(), 0x00]);
        assert_eq!(packed(Sync::new()), [Tag::Sync.as_u8(), 0x00]);
    }

    #[test]
    fn firm_info_encodes_the_info_type() {
        for ty in [
            InfoType::CpuMajor,
            InfoType::CpuMinor,
            InfoType::FpgaMajor,
            InfoType::FpgaMinor,
            InfoType::FpgaFunctions,
            InfoType::Clear,
        ] {
            assert_eq!(
                packed(FirmInfo::new(ty)),
                [Tag::FirmInfo.as_u8(), ty.as_u8()]
            );
        }
    }

    #[test]
    fn bool_ops_encode_zero_or_one() {
        for value in [false, true] {
            assert_eq!(
                packed(ForceFan::new(value)),
                [Tag::ForceFan.as_u8(), u8::from(value)]
            );
            assert_eq!(
                packed(ReadsFpgaState::new(value)),
                [Tag::ReadsFpgaState.as_u8(), u8::from(value)]
            );
        }
    }
}
