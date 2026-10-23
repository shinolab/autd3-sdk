pub use autd3_cpu_wire::config::{
    CpuConfig, FpgaBusWait, MaxFreqPpb, PathDelayFilterShift, PtpConfig,
};
use autd3_cpu_wire::payload::SetCpuConfigPayload;

use crate::datagram::Datagram;
use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::Cmd;

use super::{Operation, encode_header_only};

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
    fn encode(&self, _device: &Device, out: &mut Datagram) -> Result<(), Error> {
        let payload = SetCpuConfigPayload::encode(&self.config)?;
        encode_header_only(out, Cmd::SetCpuConfig, &payload);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;
    use crate::commands::operation::Distribution;
    use crate::error::PayloadError;
    use crate::test_utils::encode;
    use autd3_cpu_wire::payload::CpuConfigOutOfRange;

    #[test]
    fn the_config_is_encoded_as_a_broadcast_payload() {
        let config = CpuConfig {
            sync_guard: Duration::ZERO,
            ..CpuConfig::default()
        };
        let (encoded, out) = encode(&SetCpuConfig::new(config)).unwrap();

        assert_eq!(
            encoded,
            (Cmd::SetCpuConfig, size_of::<SetCpuConfigPayload>())
        );
        assert_eq!(SetCpuConfigPayload::parse(&out[..encoded.1]), Ok(config));
        assert_eq!(
            SetCpuConfig::default().distribution(),
            Distribution::Broadcast
        );
    }

    #[test]
    fn an_enabled_failsafe_is_carried_as_some() {
        let config = CpuConfig {
            failsafe_timeout: Some(Duration::from_millis(500)),
            ..CpuConfig::default()
        };
        let (encoded, out) = encode(&SetCpuConfig::new(config)).unwrap();
        assert_eq!(SetCpuConfigPayload::parse(&out[..encoded.1]), Ok(config));
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
            Error::InvalidPayload(PayloadError::CpuConfig(CpuConfigOutOfRange {
                field: "failsafe_timeout",
                ..
            }))
        ));
        assert_eq!(
            err.to_string(),
            "invalid payload: CPU config `failsafe_timeout` = 0ns must be a multiple of 1ms within 1ms..=65.535s"
        );
    }

    #[test]
    fn a_duration_the_wire_cannot_carry_is_a_payload_error() {
        let err = encode(&SetCpuConfig::new(CpuConfig {
            cpu_watchdog_timeout: Some(Duration::from_micros(1500)),
            ..CpuConfig::default()
        }))
        .unwrap_err();

        assert!(matches!(
            err,
            Error::InvalidPayload(PayloadError::CpuConfig(CpuConfigOutOfRange {
                field: "cpu_watchdog_timeout",
                ..
            }))
        ));
    }
}
