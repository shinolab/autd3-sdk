use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::config::FpgaConfig;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SetFpgaConfigPayload {
    pub sync_phase_lock: bool,
}

impl SetFpgaConfigPayload {
    #[must_use]
    pub const fn encode(config: &FpgaConfig) -> Self {
        Self {
            sync_phase_lock: config.sync_phase_lock,
        }
    }

    pub fn parse(payload: &[u8]) -> Result<FpgaConfig, Error> {
        let p = try_read_exact::<Self>(payload)?;
        Ok(FpgaConfig {
            sync_phase_lock: p.sync_phase_lock,
        })
    }
}

const _: () = assert!(core::mem::size_of::<SetFpgaConfigPayload>() == 1);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_round_trips_through_the_payload() {
        for sync_phase_lock in [false, true] {
            let config = FpgaConfig { sync_phase_lock };
            let payload = SetFpgaConfigPayload::encode(&config);
            assert_eq!(SetFpgaConfigPayload::parse(payload.as_bytes()), Ok(config));
        }
    }

    #[test]
    fn a_payload_of_the_wrong_length_or_value_is_rejected() {
        assert_eq!(SetFpgaConfigPayload::parse(&[]), Err(Error::InvalidPayload));
        assert_eq!(
            SetFpgaConfigPayload::parse(&[1, 0]),
            Err(Error::InvalidPayload)
        );
        assert_eq!(
            SetFpgaConfigPayload::parse(&[2]),
            Err(Error::InvalidPayload)
        );
    }
}
