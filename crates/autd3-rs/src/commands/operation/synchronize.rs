use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

#[derive(Clone, Copy, Debug)]
pub struct Synchronize;

impl Operation for Synchronize {
    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(&self, _device: &Device, _out: &mut [u8; PAYLOAD_BYTES]) -> Result<Cmd, Error> {
        Ok(Cmd::Synchronize)
    }
}
