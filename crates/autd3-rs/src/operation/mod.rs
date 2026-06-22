mod change_mod_bank;
mod change_pattern_bank;
mod config_modulation;
mod config_pattern;
mod group;
mod synchronize;
mod write_foci_buffer;
mod write_modulation_buffer;
mod write_pattern_buffer;
mod xor_hash;

pub use change_mod_bank::ChangeModulationBank;
pub use change_pattern_bank::ChangePatternBank;
pub use config_modulation::ConfigModulation;
pub use config_pattern::ConfigPattern;
pub use group::Group;
pub use synchronize::Synchronize;
pub use write_foci_buffer::WriteFociBuffer;
pub use write_modulation_buffer::WriteModulationBuffer;
pub use write_pattern_buffer::WritePatternBuffer;
pub use xor_hash::{XOR_HASH_HEADER_BYTES, XOR_HASH_MAX_DATA_LEN, XorHashCmd};

use crate::error::Error;
use crate::params::FOCUS_WORDS;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Distribution {
    Broadcast,
    PerDevice,
}

pub trait Operation {
    fn frames(&self) -> usize;

    fn distribution(&self) -> Distribution;

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error>;
}

impl<T: Operation + ?Sized> Operation for &T {
    fn frames(&self) -> usize {
        (**self).frames()
    }

    fn distribution(&self) -> Distribution {
        (**self).distribution()
    }

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        (**self).encode(device, frame, out)
    }
}

pub const WRITE_HEADER_BYTES: usize = 8;

pub const WRITE_MAX_DATA_LEN: usize = PAYLOAD_BYTES - WRITE_HEADER_BYTES;

pub const MAX_FOCI_PER_FRAME: usize = WRITE_MAX_DATA_LEN / (FOCUS_WORDS * 2);
