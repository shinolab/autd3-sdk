use core::num::NonZeroU16;
use core::time::Duration;

use autd3_cpu_wire::payload::SilencerPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::U16;

use crate::common::{ULTRASOUND_FREQ, ULTRASOUND_PERIOD};
use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::mirror::FirmwareState;
use crate::params::{
    SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY, SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
    SILENCER_DEFAULT_UPDATE_RATE, SILENCER_FLAG_FIXED_UPDATE_RATE_MODE, SILENCER_FLAG_STRICT_MODE,
};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

fn write_payload(
    out: &mut [u8; PAYLOAD_BYTES],
    flag: u8,
    update_rate_intensity: u16,
    update_rate_phase: u16,
    completion_steps_intensity: u16,
    completion_steps_phase: u16,
) {
    let (p, _) = SilencerPayload::mut_from_prefix(&mut out[..]).unwrap();
    *p = SilencerPayload {
        flag,
        reserved: 0,
        update_rate_intensity: U16::new(update_rate_intensity),
        update_rate_phase: U16::new(update_rate_phase),
        completion_steps_intensity: U16::new(completion_steps_intensity),
        completion_steps_phase: U16::new(completion_steps_phase),
    };
}

fn completion_time_to_steps(value: Duration) -> Result<u16, Error> {
    const NANOSEC: u128 = 1_000_000_000;
    let v = value.as_nanos() * u128::from(ULTRASOUND_FREQ.hz());
    if !v.is_multiple_of(NANOSEC) {
        return Err(PayloadError::SilencerCompletionTimeNotMultiple(value).into());
    }
    let steps = v / NANOSEC;
    if steps == 0 {
        return Err(PayloadError::SilencerCompletionTimeOutOfRange(value).into());
    }
    u16::try_from(steps).map_err(|_| PayloadError::SilencerCompletionTimeOutOfRange(value).into())
}

mod sealed {
    pub trait Sealed {}
}

pub trait SilencerConfig: sealed::Sealed + Copy {
    #[doc(hidden)]
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error>;

    #[doc(hidden)]
    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedCompletionTime {
    pub intensity: Duration,
    pub phase: Duration,
    pub strict_mode: bool,
}

impl Default for FixedCompletionTime {
    fn default() -> Self {
        Self {
            intensity: ULTRASOUND_PERIOD * u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY),
            phase: ULTRASOUND_PERIOD * u32::from(SILENCER_DEFAULT_COMPLETION_STEPS_PHASE),
            strict_mode: true,
        }
    }
}

impl sealed::Sealed for FixedCompletionTime {}
impl SilencerConfig for FixedCompletionTime {
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        let intensity = completion_time_to_steps(self.intensity)?;
        let phase = completion_time_to_steps(self.phase)?;
        let flag = if self.strict_mode {
            SILENCER_FLAG_STRICT_MODE
        } else {
            0
        };
        write_payload(
            out,
            flag,
            SILENCER_DEFAULT_UPDATE_RATE,
            SILENCER_DEFAULT_UPDATE_RATE,
            intensity,
            phase,
        );
        Ok(Cmd::SetSilencer)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        let intensity = completion_time_to_steps(self.intensity)?;
        let phase = completion_time_to_steps(self.phase)?;
        if self.strict_mode {
            state.silencer.check_set_strict(device, intensity, phase)?;
        }
        state
            .silencer
            .apply_completion(intensity, phase, self.strict_mode);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedUpdateRate {
    pub intensity: NonZeroU16,
    pub phase: NonZeroU16,
}

impl sealed::Sealed for FixedUpdateRate {}
impl SilencerConfig for FixedUpdateRate {
    fn write_payload(&self, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        write_payload(
            out,
            SILENCER_FLAG_FIXED_UPDATE_RATE_MODE,
            self.intensity.get(),
            self.phase.get(),
            SILENCER_DEFAULT_COMPLETION_STEPS_INTENSITY,
            SILENCER_DEFAULT_COMPLETION_STEPS_PHASE,
        );
        Ok(Cmd::SetSilencer)
    }

    fn reflect(&self, _device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        state.silencer.release();
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSilencer<T: SilencerConfig> {
    pub config: T,
}

impl<T: SilencerConfig> SetSilencer<T> {
    #[must_use]
    pub const fn new(config: T) -> Self {
        Self { config }
    }
}

impl SetSilencer<FixedCompletionTime> {
    #[must_use]
    pub const fn disable() -> Self {
        Self::new(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD,
            phase: ULTRASOUND_PERIOD,
            strict_mode: false,
        })
    }
}

impl Default for SetSilencer<FixedCompletionTime> {
    fn default() -> Self {
        Self::new(FixedCompletionTime::default())
    }
}

impl<T: SilencerConfig> Operation for SetSilencer<T> {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: &Device,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        self.config.write_payload(out)
    }

    fn reflect(&self, device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        self.config.reflect(device, state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    fn encode<T: SilencerConfig>(config: T) -> Result<(Cmd, [u8; PAYLOAD_BYTES]), Error> {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetSilencer::new(config).encode(&test_device(0), 0, &mut out)?;
        Ok((cmd, out))
    }

    fn nz(v: u16) -> NonZeroU16 {
        NonZeroU16::new(v).unwrap()
    }

    #[test]
    fn fixed_completion_time_lays_out_fields() {
        let (cmd, payload) = encode(FixedCompletionTime {
            intensity: ULTRASOUND_PERIOD * 5,
            phase: ULTRASOUND_PERIOD * 7,
            strict_mode: true,
        })
        .unwrap();

        assert_eq!(cmd, Cmd::SetSilencer);
        assert_eq!(payload[0], SILENCER_FLAG_STRICT_MODE);
        assert_eq!(payload[1], 0);
        assert_eq!(&payload[2..4], &SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[4..6], &SILENCER_DEFAULT_UPDATE_RATE.to_le_bytes());
        assert_eq!(&payload[6..8], &5u16.to_le_bytes());
        assert_eq!(&payload[8..10], &7u16.to_le_bytes());
        assert!(payload[10..].iter().all(|&b| b == 0));
    }

    #[test]
    fn fixed_completion_time_default_is_10_40_strict() {
        let (_cmd, payload) = encode(FixedCompletionTime::default()).unwrap();
        assert_eq!(payload[0], SILENCER_FLAG_STRICT_MODE);
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn silencer_default_is_fixed_completion_time_default() {
        let mut out = [0u8; PAYLOAD_BYTES];
        SetSilencer::default()
            .encode(&test_device(0), 0, &mut out)
            .unwrap();
        assert_eq!(out[0], SILENCER_FLAG_STRICT_MODE);
        assert_eq!(&out[6..8], &10u16.to_le_bytes());
        assert_eq!(&out[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn disable_is_one_step_non_strict() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetSilencer::disable()
            .encode(&test_device(0), 0, &mut out)
            .unwrap();
        assert_eq!(cmd, Cmd::SetSilencer);
        assert_eq!(out[0], 0);
        assert_eq!(&out[6..8], &1u16.to_le_bytes());
        assert_eq!(&out[8..10], &1u16.to_le_bytes());
    }

    #[test]
    fn fixed_completion_time_non_strict_clears_flag() {
        let (_cmd, payload) = encode(FixedCompletionTime {
            strict_mode: false,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(payload[0], 0);
    }

    #[test]
    fn fixed_update_rate_sets_mode_flag() {
        let (cmd, payload) = encode(FixedUpdateRate {
            intensity: nz(8),
            phase: nz(16),
        })
        .unwrap();

        assert_eq!(cmd, Cmd::SetSilencer);
        assert_eq!(payload[0], SILENCER_FLAG_FIXED_UPDATE_RATE_MODE);
        assert_eq!(&payload[2..4], &8u16.to_le_bytes());
        assert_eq!(&payload[4..6], &16u16.to_le_bytes());
        assert_eq!(&payload[6..8], &10u16.to_le_bytes());
        assert_eq!(&payload[8..10], &40u16.to_le_bytes());
    }

    #[test]
    fn rejects_non_multiple_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD + Duration::from_nanos(1),
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeNotMultiple(_)
            ))
        ));
    }

    #[test]
    fn rejects_zero_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: Duration::ZERO,
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeOutOfRange(_)
            ))
        ));
    }

    #[test]
    fn rejects_out_of_range_completion_time() {
        assert!(matches!(
            encode(FixedCompletionTime {
                intensity: ULTRASOUND_PERIOD * 65536,
                phase: ULTRASOUND_PERIOD,
                strict_mode: true,
            }),
            Err(Error::InvalidPayload(
                PayloadError::SilencerCompletionTimeOutOfRange(_)
            ))
        ));
    }
}
