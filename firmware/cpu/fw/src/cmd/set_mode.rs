pub use autd3_cpu_wire::payload::SetModePayload;

use crate::app::Cpu;
use crate::proto::Error;

impl Cpu {
    pub(crate) fn set_mode_cmd(&self, payload: &[u8]) -> Result<(), Error> {
        self.set_mode(SetModePayload::parse(payload)?.mode);
        Ok(())
    }
}
