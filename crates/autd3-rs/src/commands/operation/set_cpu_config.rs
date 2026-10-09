pub use autd3_cpu_wire::config::{CpuConfig, FpgaBusWait, PtpConfig};
use autd3_cpu_wire::payload::{CpuConfigOutOfRange, SetCpuConfigPayload};

use crate::error::{Error, PayloadError};
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation, encode_fixed};

impl From<CpuConfigOutOfRange> for PayloadError {
    fn from(e: CpuConfigOutOfRange) -> Self {
        PayloadError::CpuConfigOutOfRange {
            field: e.field,
            value: e.value,
            unit: e.unit,
            min: e.min,
            max: e.max,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SetCpuConfig {
    pub config: CpuConfig,
}

impl SetCpuConfig {
    #[must_use]
    pub const fn new(config: CpuConfig) -> Self {
        Self { config }
    }
}

impl crate::sealed::Sealed for SetCpuConfig {}

impl Operation for SetCpuConfig {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        let payload = SetCpuConfigPayload::encode(&self.config)?;
        Ok(encode_fixed(out, Cmd::SetCpuConfig, &payload))
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::commands::operation::Distribution;
    use crate::test_utils::encode;

    #[test]
    fn the_config_is_encoded_as_a_broadcast_payload() {
        let config = CpuConfig {
            sys_time_transition_margin: Duration::ZERO,
            ..CpuConfig::default()
        };
        let (encoded, out) = encode(&SetCpuConfig::new(config)).unwrap();

        assert_eq!(
            encoded,
            Encoded::new(Cmd::SetCpuConfig, size_of::<SetCpuConfigPayload>())
        );
        assert_eq!(SetCpuConfigPayload::parse(&out[..encoded.len]), Ok(config));
        assert_eq!(
            SetCpuConfig::default().distribution(),
            Distribution::Broadcast
        );
    }

    #[test]
    fn a_disabled_failsafe_is_carried_as_none() {
        let config = CpuConfig {
            failsafe_timeout: None,
            ..CpuConfig::default()
        };
        let (encoded, out) = encode(&SetCpuConfig::new(config)).unwrap();
        assert_eq!(SetCpuConfigPayload::parse(&out[..encoded.len]), Ok(config));
    }

    #[test]
    fn a_zero_failsafe_timeout_is_a_payload_error() {
        let err = encode(&SetCpuConfig::new(CpuConfig {
            failsafe_timeout: Some(Duration::ZERO),
            ..CpuConfig::default()
        }))
        .unwrap_err();

        assert!(matches!(
            err,
            Error::InvalidPayload(PayloadError::CpuConfigOutOfRange {
                field: "failsafe_timeout",
                ..
            })
        ));
        assert_eq!(
            err.to_string(),
            "invalid payload: CPU config `failsafe_timeout` = 0ns must be a multiple of 1ms within 1ms..=65.535s"
        );
    }

    #[test]
    fn a_duration_the_wire_cannot_carry_is_a_payload_error() {
        let err = encode(&SetCpuConfig::new(CpuConfig {
            update_activate_delay: Duration::from_micros(1500),
            ..CpuConfig::default()
        }))
        .unwrap_err();

        assert!(matches!(
            err,
            Error::InvalidPayload(PayloadError::CpuConfigOutOfRange {
                field: "update_activate_delay",
                ..
            })
        ));
    }
}
