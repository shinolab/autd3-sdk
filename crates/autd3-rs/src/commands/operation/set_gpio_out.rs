use autd3_cpu_wire::payload::GpioOutPayload;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Encoded, Operation, encode_fixed};

pub use autd3_cpu_wire::value::GpioOut;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SetGpioOut {
    pub outputs: [GpioOut; 4],
}

impl crate::sealed::Sealed for SetGpioOut {}

impl Operation for SetGpioOut {
    fn encode(&self, _device: &Device, out: &mut [u8; PAYLOAD_BYTES]) -> Result<Encoded, Error> {
        Ok(encode_fixed(
            out,
            Cmd::SetGpioOut,
            &GpioOutPayload::new(self.outputs),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::SysTime;
    use autd3_cpu_wire::fpga_params::GpioOutType;

    const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;
    use crate::test_utils::encode;

    #[test]
    fn gpio_out_encodes_tag_and_value() {
        let (cmd, out) = encode(&SetGpioOut {
            outputs: [
                GpioOut::Off,
                GpioOut::Direct(true),
                GpioOut::PwmOut(7),
                GpioOut::ModIdx(0x1234),
            ],
        })
        .unwrap();
        assert_eq!(
            cmd,
            Encoded::new(Cmd::SetGpioOut, size_of::<GpioOutPayload>())
        );
        assert_eq!(&out[0..8], &0u64.to_le_bytes());
        assert_eq!(
            &out[8..16],
            &((u64::from(GpioOutType::Direct.as_u8()) << 56) | 1).to_le_bytes()
        );
        assert_eq!(
            &out[16..24],
            &((u64::from(GpioOutType::PwmOut.as_u8()) << 56) | 7).to_le_bytes()
        );
        assert_eq!(
            &out[24..32],
            &((u64::from(GpioOutType::ModIdx.as_u8()) << 56) | 0x1234).to_le_bytes()
        );
    }

    #[test]
    fn sys_time_eq_encodes_scaled_fpga_value() {
        let ec_time_ns = 0x0123_4567_89AB_CDEFu64;
        let expected = ((ec_time_ns / 3125) << 6) >> 9;
        let (_, out) = encode(&SetGpioOut {
            outputs: [
                GpioOut::Off,
                GpioOut::SysTimeEq(SysTime::from_nanos(ec_time_ns)),
                GpioOut::Off,
                GpioOut::Off,
            ],
        })
        .unwrap();
        assert_eq!(
            &out[8..16],
            &((u64::from(GpioOutType::SysTimeEq.as_u8()) << 56) | (expected & VALUE_MASK))
                .to_le_bytes()
        );
    }
}
