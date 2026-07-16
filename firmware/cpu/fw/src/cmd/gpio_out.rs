use core::mem::{offset_of, size_of};

use zerocopy::little_endian::U64;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, Unaligned};

use crate::app::Cpu;
use crate::fpga;
use crate::params::{ADDR_DEBUG_VALUE0_0, CTL_FLAG_DEBUG_SET};
use crate::port::Port;
use crate::proto::{Error, PAYLOAD_BYTES};

pub const GPIO_OUT_NUM: usize = 4;

#[derive(FromBytes, IntoBytes, KnownLayout, Immutable, Unaligned)]
#[repr(C)]
pub struct GpioOutPayload {
    pub values: [U64; GPIO_OUT_NUM],
}

const _: () = assert!(size_of::<GpioOutPayload>() <= PAYLOAD_BYTES);
const _: () = assert!(offset_of!(GpioOutPayload, values) == 0);

impl Cpu {
    pub(crate) fn gpio_out<P: Port>(&self, port: &mut P, payload: &[u8]) -> Result<(), Error> {
        let Ok((p, _)) = GpioOutPayload::ref_from_prefix(payload) else {
            return Err(Error::InvalidPayload);
        };
        for (i, value) in p.values.iter().enumerate() {
            fpga::write_u64(port, ADDR_DEBUG_VALUE0_0 + 4 * i as u16, value.get());
        }
        self.set_and_wait_update(port, CTL_FLAG_DEBUG_SET)
    }
}
