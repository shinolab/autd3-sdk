use zerocopy::FromBytes;

pub use autd3_cpu_wire::payload::SetModePayload;

use crate::app::Cpu;
use crate::proto::{Error, Mode};

impl Cpu {
    pub(crate) fn set_mode_cmd(&self, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = SetModePayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        let Some(mode) = Mode::from_u8(p.mode) else {
            return Err(Error::InvalidPayload);
        };
        self.set_mode(mode);
        Ok(())
    }
}
