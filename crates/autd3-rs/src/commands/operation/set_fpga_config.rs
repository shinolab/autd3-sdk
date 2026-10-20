pub use autd3_cpu_wire::config::FpgaConfig;
use autd3_cpu_wire::payload::SetFpgaConfigPayload;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation, encode_header_only};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SetFpgaConfig {
    pub config: FpgaConfig,
}

impl SetFpgaConfig {
    #[must_use]
    pub const fn new(config: FpgaConfig) -> Self {
        Self { config }
    }
}

impl crate::sealed::Sealed for SetFpgaConfig {}

impl Operation for SetFpgaConfig {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(encode_header_only(
            out,
            Cmd::SetFpgaConfig,
            &SetFpgaConfigPayload::encode(&self.config),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::encode;

    #[test]
    fn the_default_enables_the_sync_phase_lock() {
        assert!(SetFpgaConfig::default().config.sync_phase_lock);
    }

    #[test]
    fn the_config_is_encoded() {
        let (cmd, out) = encode(&SetFpgaConfig::default()).unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::SetFpgaConfig, size_of::<SetFpgaConfigPayload>())
        );
        assert_eq!(out[0], 1);

        let (_, out) = encode(&SetFpgaConfig::new(FpgaConfig {
            sync_phase_lock: false,
        }))
        .unwrap();
        assert_eq!(out[0], 0);
    }
}
