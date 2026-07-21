use crate::error::Error;
use crate::mirror::FirmwareState;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug, Default)]
pub struct Clear;

impl Operation for Clear {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: usize,
        _frame: usize,
        _out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        Ok(Cmd::Clear)
    }

    fn reflect(&self, _device: usize, state: &mut FirmwareState) -> Result<(), Error> {
        *state = FirmwareState::boot_default();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_is_no_payload_broadcast() {
        let mut out = [0xAAu8; PAYLOAD_BYTES];
        let cmd = Clear.encode(0, 0, &mut out).unwrap();
        assert_eq!(cmd, Cmd::Clear);
        assert_eq!(Clear.distribution(), Distribution::Broadcast);
        assert_eq!(Clear.frames(), 1);
    }

    #[test]
    fn clear_resets_mirror_to_boot_default() {
        let mut state = FirmwareState::boot_default();
        state.silencer.apply_completion(10, 40, true);
        state.silencer.note_mod_div(0, 5);
        Clear.reflect(0, &mut state).unwrap();
        assert_eq!(state, FirmwareState::boot_default());
    }
}
