use zerocopy::little_endian::U16;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes, Unaligned};

use super::try_read_exact;
use crate::Error;
use crate::config::FpgaConfig;

#[derive(TryFromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct SetFpgaConfigPayload {
    pub sync_phase_lock: bool,
    pub sync_kp: U16,
    pub sync_ki: U16,
    pub sync_kf: U16,
}

impl SetFpgaConfigPayload {
    #[must_use]
    pub const fn encode(config: &FpgaConfig) -> Self {
        Self {
            sync_phase_lock: config.sync_phase_lock,
            sync_kp: U16::new(config.sync_kp),
            sync_ki: U16::new(config.sync_ki),
            sync_kf: U16::new(config.sync_kf),
        }
    }

    pub fn parse(payload: &[u8]) -> Result<FpgaConfig, Error> {
        let p = try_read_exact::<Self>(payload)?;
        Ok(FpgaConfig {
            sync_phase_lock: p.sync_phase_lock,
            sync_kp: p.sync_kp.get(),
            sync_ki: p.sync_ki.get(),
            sync_kf: p.sync_kf.get(),
        })
    }
}

assert_layout!(SetFpgaConfigPayload { sync_phase_lock: 0, sync_kp: 1, sync_ki: 3, sync_kf: 5 } size 7);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_round_trips_through_the_payload() {
        for sync_phase_lock in [false, true] {
            let config = FpgaConfig {
                sync_phase_lock,
                sync_kp: 0x0102,
                sync_ki: 0,
                sync_kf: u16::MAX,
            };
            let payload = SetFpgaConfigPayload::encode(&config);
            assert_eq!(SetFpgaConfigPayload::parse(payload.as_bytes()), Ok(config));
        }
    }

    #[test]
    fn the_gains_are_little_endian_after_the_flag() {
        let payload = SetFpgaConfigPayload::encode(&FpgaConfig {
            sync_phase_lock: true,
            sync_kp: 0x0102,
            sync_ki: 0x0304,
            sync_kf: 0x0506,
        });
        assert_eq!(payload.as_bytes(), [1, 0x02, 0x01, 0x04, 0x03, 0x06, 0x05]);
    }

    #[test]
    fn a_payload_of_the_wrong_length_or_value_is_rejected() {
        assert_eq!(SetFpgaConfigPayload::parse(&[]), Err(Error::InvalidPayload));
        assert_eq!(
            SetFpgaConfigPayload::parse(&[1]),
            Err(Error::InvalidPayload)
        );
        assert_eq!(
            SetFpgaConfigPayload::parse(&[1, 0, 0, 0, 0, 0, 0, 0]),
            Err(Error::InvalidPayload)
        );
        assert_eq!(
            SetFpgaConfigPayload::parse(&[2, 0, 0, 0, 0, 0, 0]),
            Err(Error::InvalidPayload)
        );
    }
}
