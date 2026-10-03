use autd3_cpu_wire::payload::WriteModulationFusedPayload;
use zerocopy::little_endian::{U16, U32, U64};

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::params::{BUFFER_SIZE_MIN, MOD_BUFFER_SAMPLES};
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::{LoopBehavior, ModulationBank, SamplingConfig, TransitionMode};

use super::{Distribution, Encoded, Operation, write_header};

#[derive(Clone, Copy, Debug)]
pub struct WriteModulationFused<'a> {
    pub bank: ModulationBank,
    pub data: &'a [u8],
    pub config: SamplingConfig,
    pub loop_behavior: LoopBehavior,
    pub transition_mode: TransitionMode,
}

impl WriteModulationFused<'_> {
    #[must_use]
    pub fn fits_single_frame(len: usize) -> bool {
        len > 0 && len <= PAYLOAD_BYTES - size_of::<WriteModulationFusedPayload>()
    }
}

impl crate::sealed::Sealed for WriteModulationFused<'_> {}

impl Operation for WriteModulationFused<'_> {
    fn apply_clock_offset(&mut self, offset_ns: i64) {
        self.transition_mode = self.transition_mode.with_clock_offset(offset_ns);
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        if self.data.len() < BUFFER_SIZE_MIN {
            return Err(PayloadError::ModulationSizeOutOfRange {
                size: self.data.len(),
                min: BUFFER_SIZE_MIN,
                max: MOD_BUFFER_SAMPLES,
            }
            .into());
        }
        let capacity = PAYLOAD_BYTES - size_of::<WriteModulationFusedPayload>();
        if self.data.len() > capacity {
            return Err(PayloadError::ModulationWriteExceedsCapacity {
                offset: 0,
                end: self.data.len(),
                capacity,
            }
            .into());
        }
        if self.data.len() > MOD_BUFFER_SAMPLES {
            return Err(PayloadError::ModulationSizeOutOfRange {
                size: self.data.len(),
                min: BUFFER_SIZE_MIN,
                max: MOD_BUFFER_SAMPLES,
            }
            .into());
        }
        let divider = self.config.divide()?;
        let margin_ns = self.transition_mode.margin_ns()?;

        let rest = write_header(
            out,
            &WriteModulationFusedPayload {
                bank: self.bank,
                transition_mode: self.transition_mode.try_as_wire()?,
                divider: U16::new(divider),
                size: U32::new(
                    u32::try_from(self.data.len()).expect("bounded by MOD_BUFFER_SAMPLES"),
                ),
                rep: U16::new(self.loop_behavior.rep()),
                transition_value: U64::new(self.transition_mode.value()),
                margin_ns: U32::new(margin_ns),
            },
        );
        rest[..self.data.len()].copy_from_slice(self.data);
        Ok(Encoded::header_with_data::<WriteModulationFusedPayload>(
            Cmd::WriteModulationFused,
            self.data.len(),
        ))
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let divider = self.config.divide()?;
        let bank = self.bank.as_u8();
        state.silencer.check_mod_div(device, divider)?;
        state.silencer.note_mod_div(bank, divider);
        state.transition.note_mod_loop(bank, self.loop_behavior);

        state.silencer.check_mod_bank(device, bank)?;
        state
            .transition
            .check_mod_bank(device, bank, self.transition_mode)?;
        state.silencer.note_mod_bank(bank);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;
    use core::num::NonZeroU16;

    #[test]
    fn fused_modulation_lays_out_header_and_data() {
        let data = [0xAA, 0xBB, 0xCC, 0xDD];
        let op = WriteModulationFused {
            bank: ModulationBank::B1,
            data: &data,
            config: SamplingConfig::new(NonZeroU16::new(10).unwrap()),
            loop_behavior: LoopBehavior::Finite(NonZeroU16::new(10).unwrap()),
            transition_mode: TransitionMode::Immediate,
        };

        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = op.encode(&test_device(0), &mut out).unwrap();

        assert_eq!(
            cmd,
            Encoded::header_with_data::<WriteModulationFusedPayload>(Cmd::WriteModulationFused, 4)
        );
        assert_eq!(out[0], 1, "bank B1");
        assert_eq!(out[1], 0xFF, "IMMEDIATE");
        assert_eq!(&out[2..4], &10u16.to_le_bytes(), "divider");
        assert_eq!(&out[4..8], &4u32.to_le_bytes(), "size");
        assert_eq!(&out[8..10], &9u16.to_le_bytes(), "Finite(10) => rep 9");
        assert_eq!(&out[size_of::<WriteModulationFusedPayload>()..][..4], &data);
    }

    #[test]
    fn fused_modulation_rejects_more_than_one_frame() {
        let capacity = PAYLOAD_BYTES - size_of::<WriteModulationFusedPayload>();
        let data = vec![0x80u8; capacity + 1];
        let op = WriteModulationFused {
            bank: ModulationBank::B0,
            data: &data,
            config: SamplingConfig::FREQ_4K,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(_))
        ));

        assert!(!WriteModulationFused::fits_single_frame(capacity + 1));
        assert!(WriteModulationFused::fits_single_frame(capacity));
        assert!(!WriteModulationFused::fits_single_frame(0));
    }

    #[test]
    fn fused_modulation_rejects_empty_data() {
        let op = WriteModulationFused {
            bank: ModulationBank::B0,
            data: &[],
            config: SamplingConfig::FREQ_4K,
            loop_behavior: LoopBehavior::Infinite,
            transition_mode: TransitionMode::Immediate,
        };
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            op.encode(&test_device(0), &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
