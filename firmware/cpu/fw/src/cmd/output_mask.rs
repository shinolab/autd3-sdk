use core::mem::{offset_of, size_of};

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga;
use crate::params::{BRAM_CNT_SELECT_OUTPUT_MASK, BRAM_SELECT_CONTROLLER};
use crate::port::Port;
use crate::proto::{Error, OUTPUT_MASK_WORDS, PAYLOAD_BYTES};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct OutputMaskPayload {
    pub data: [U16; OUTPUT_MASK_WORDS],
}

const _: () = assert!(size_of::<OutputMaskPayload>() <= PAYLOAD_BYTES);
const _: () = assert!(offset_of!(OutputMaskPayload, data) == 0);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = OutputMaskPayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    for (j, value) in p.data.iter().enumerate() {
        fpga::write(
            port,
            BRAM_SELECT_CONTROLLER,
            (u16::from(BRAM_CNT_SELECT_OUTPUT_MASK) << 8) | j as u16,
            value.get(),
        );
    }
    Ok(())
}
