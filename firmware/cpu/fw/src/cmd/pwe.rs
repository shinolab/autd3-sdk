use core::mem::{offset_of, size_of};

use zerocopy::little_endian::U16;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::fpga::{self, PWE_TABLE_SIZE};
use crate::params::BRAM_SELECT_PWE_TABLE;
use crate::port::Port;
use crate::proto::{Error, PAYLOAD_BYTES};

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct PwePayload {
    pub table: [U16; PWE_TABLE_SIZE],
}

const _: () = assert!(size_of::<PwePayload>() <= PAYLOAD_BYTES);
const _: () = assert!(offset_of!(PwePayload, table) == 0);

pub(crate) fn handle<P: Port>(port: &mut P, payload: &[u8]) -> Result<(), Error> {
    let Ok((p, _)) = PwePayload::ref_from_prefix(payload) else {
        return Err(Error::InvalidPayload);
    };
    for (i, value) in p.table.iter().enumerate() {
        fpga::write(port, BRAM_SELECT_PWE_TABLE, i as u16, value.get());
    }
    Ok(())
}
