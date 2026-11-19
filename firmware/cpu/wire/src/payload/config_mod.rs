use core::num::NonZeroU16;

use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_exact};
use crate::layout::{BUFFER_SIZE_MIN, MOD_BUFFER_SAMPLES};
use crate::value::{LoopBehavior, SamplingConfig};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    pub bank: ModulationBank,
    pub reserved: u8,
    pub divider: U16,
    pub size: U32,
    pub rep: U16,
}

impl ConfigModPayload {
    pub fn new(
        bank: ModulationBank,
        config: SamplingConfig,
        size: usize,
        loop_behavior: LoopBehavior,
    ) -> Result<Self, PayloadBuildError> {
        if !(BUFFER_SIZE_MIN..=MOD_BUFFER_SAMPLES).contains(&size) {
            return Err(PayloadBuildError::ModulationSizeOutOfRange {
                size,
                min: BUFFER_SIZE_MIN,
                max: MOD_BUFFER_SAMPLES,
            });
        }
        Ok(Self {
            bank,
            reserved: 0,
            divider: U16::new(config.divide().get()),
            size: U32::new(size as u32),
            rep: U16::new(loop_behavior.rep()),
        })
    }

    #[must_use]
    pub const fn divider(&self) -> Option<NonZeroU16> {
        NonZeroU16::new(self.divider.get())
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        p.divider().ok_or(Error::InvalidPayload)?;
        if !(BUFFER_SIZE_MIN as u32..=MOD_BUFFER_SAMPLES as u32).contains(&p.size.get()) {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }
}

assert_layout!(ConfigModPayload {
    bank: 0,
    reserved: 1,
    divider: 2,
    size: 4,
    rep: 8,
} size 10);

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use zerocopy::IntoBytes;

    use super::*;

    #[rstest]
    #[case(BUFFER_SIZE_MIN)]
    #[case(MOD_BUFFER_SAMPLES)]
    fn a_built_payload_passes_parse(#[case] size: usize) {
        let built = ConfigModPayload::new(
            ModulationBank::B1,
            SamplingConfig::FREQ_4K,
            size,
            LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
        )
        .unwrap();
        let parsed = ConfigModPayload::parse(built.as_bytes()).unwrap();
        assert_eq!(parsed.bank, ModulationBank::B1);
        assert_eq!(parsed.divider.get(), 10);
        assert_eq!(parsed.size.get() as usize, size);
        assert_eq!(parsed.rep.get(), 9);
    }

    #[rstest]
    #[case(BUFFER_SIZE_MIN - 1)]
    #[case(MOD_BUFFER_SAMPLES + 1)]
    #[case(usize::MAX)]
    fn a_size_outside_the_buffer_is_rejected(#[case] size: usize) {
        assert_eq!(
            ConfigModPayload::new(
                ModulationBank::B0,
                SamplingConfig::FREQ_40K,
                size,
                LoopBehavior::ONCE
            )
            .err(),
            Some(PayloadBuildError::ModulationSizeOutOfRange {
                size,
                min: BUFFER_SIZE_MIN,
                max: MOD_BUFFER_SAMPLES,
            })
        );
    }
}
