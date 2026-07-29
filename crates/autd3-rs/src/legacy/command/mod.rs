mod adapt;
mod change_segment;

use crate::legacy::datagram::LegacyDatagramBuilder;

pub use change_segment::LegacyChangePatternBank;

pub trait LegacyCommand<'a> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>);
}

macro_rules! op_command {
    ($($ty:ty),* $(,)?) => {
        $(
            impl<'a> LegacyCommand<'a> for $ty {
                fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
                    builder.push_op(self);
                }
            }
        )*
    };
}

op_command!(
    crate::legacy::op::Nop,
    crate::legacy::op::Clear,
    crate::legacy::op::Sync,
    crate::legacy::op::FirmInfo,
    crate::legacy::op::ForceFan,
    crate::legacy::op::ReadsFpgaState,
    crate::legacy::op::Silencer,
    crate::legacy::op::SetGpioOut,
    crate::legacy::op::EmulateGpioIn,
    crate::legacy::op::Gain<'a>,
    crate::legacy::op::SetOutputMask<'a>,
    crate::legacy::op::SetPhaseCorrection<'a>,
    crate::legacy::op::SetPulseWidthTable<'a>,
    crate::legacy::op::Modulation<'a>,
    crate::legacy::op::GainStm<'a>,
);

impl<'a, const N: usize> LegacyCommand<'a> for crate::legacy::op::FociStm<'a, N> {
    fn expand(self, builder: &mut LegacyDatagramBuilder<'a>) {
        builder.push_op(self);
    }
}
