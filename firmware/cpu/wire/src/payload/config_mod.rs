use zerocopy::little_endian::{U16, U32};
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::{PayloadBuildError, try_read_exact};
use crate::layout::{BUFFER_SIZE_MIN, MOD_BUFFER_SAMPLES};
use crate::value::{LoopBehavior, SamplingConfig};
use crate::{Error, ModulationBank};

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct ConfigModPayload {
    bank: ModulationBank,
    divider: U16,
    size: U32,
    rep: U16,
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
            divider: U16::new(config.divide().get()),
            size: U32::new(size as u32),
            rep: U16::new(loop_behavior.rep()),
        })
    }

    #[must_use]
    pub const fn bank(&self) -> ModulationBank {
        self.bank
    }

    #[must_use]
    pub const fn divider(&self) -> u16 {
        self.divider.get()
    }

    #[must_use]
    pub const fn size(&self) -> u32 {
        self.size.get()
    }

    #[must_use]
    pub const fn rep(&self) -> u16 {
        self.rep.get()
    }

    pub fn parse(payload: &[u8]) -> Result<Self, Error> {
        let p = try_read_exact::<Self>(payload)?;
        if p.divider() == 0
            || !(BUFFER_SIZE_MIN as u32..=MOD_BUFFER_SAMPLES as u32).contains(&p.size())
        {
            return Err(Error::InvalidPayload);
        }
        Ok(p)
    }
}

assert_layout!(ConfigModPayload {
    bank: 0,
    divider: 1,
    size: 3,
    rep: 7,
} size 9);

#[cfg(test)]
mod tests {
    use core::num::NonZeroU16;

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
