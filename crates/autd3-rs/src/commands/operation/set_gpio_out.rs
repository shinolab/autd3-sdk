use autd3_cpu_wire::payload::GpioOutPayload;
use zerocopy::FromBytes;
use zerocopy::little_endian::U64;

use crate::error::Error;
use crate::geometry::Device;
use crate::protocol::{Cmd, PAYLOAD_BYTES};
use crate::value::DcSysTime;

use super::{Distribution, Operation};

use autd3_cpu_wire::params::{
    GPIO_O_TYPE_BASE_SIG, GPIO_O_TYPE_DIRECT, GPIO_O_TYPE_FORCE_FAN, GPIO_O_TYPE_IS_STM_MODE,
    GPIO_O_TYPE_MOD_BANK, GPIO_O_TYPE_MOD_IDX, GPIO_O_TYPE_NONE, GPIO_O_TYPE_PATTERN_BANK,
    GPIO_O_TYPE_PATTERN_IDX, GPIO_O_TYPE_PWM_OUT, GPIO_O_TYPE_SYNC, GPIO_O_TYPE_SYNC_DIFF,
    GPIO_O_TYPE_SYS_TIME_EQ, GPIO_O_TYPE_THERMO,
};

const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

const fn ec_time_to_gpio_sys_time(ec_time_ns: u64) -> u64 {
    ((ec_time_ns / 3125) << 6) >> 9
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GpioOut {
    #[default]
    Off,
    BaseSignal,
    Thermo,
    ForceFan,
    Sync,
    ModBank,
    ModIdx(u16),
    PatternBank,
    PatternIdx(u16),
    IsStmMode,
    SysTimeEq(DcSysTime),
    SyncDiff,
    PwmOut(u8),
    Direct(bool),
}

impl GpioOut {
    fn encode(self) -> u64 {
        let (tag, value): (u8, u64) = match self {
            GpioOut::Off => (GPIO_O_TYPE_NONE, 0),
            GpioOut::BaseSignal => (GPIO_O_TYPE_BASE_SIG, 0),
            GpioOut::Thermo => (GPIO_O_TYPE_THERMO, 0),
            GpioOut::ForceFan => (GPIO_O_TYPE_FORCE_FAN, 0),
            GpioOut::Sync => (GPIO_O_TYPE_SYNC, 0),
            GpioOut::ModBank => (GPIO_O_TYPE_MOD_BANK, 0),
            GpioOut::ModIdx(idx) => (GPIO_O_TYPE_MOD_IDX, u64::from(idx)),
            GpioOut::PatternBank => (GPIO_O_TYPE_PATTERN_BANK, 0),
            GpioOut::PatternIdx(idx) => (GPIO_O_TYPE_PATTERN_IDX, u64::from(idx)),
            GpioOut::IsStmMode => (GPIO_O_TYPE_IS_STM_MODE, 0),
            GpioOut::SysTimeEq(t) => (
                GPIO_O_TYPE_SYS_TIME_EQ,
                ec_time_to_gpio_sys_time(t.sys_time()),
            ),
            GpioOut::SyncDiff => (GPIO_O_TYPE_SYNC_DIFF, 0),
            GpioOut::PwmOut(tr) => (GPIO_O_TYPE_PWM_OUT, u64::from(tr)),
            GpioOut::Direct(on) => (GPIO_O_TYPE_DIRECT, u64::from(on)),
        };
        (u64::from(tag) << 56) | (value & VALUE_MASK)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SetGpioOut {
    pub outputs: [GpioOut; 4],
}

impl Operation for SetGpioOut {
    fn frames(&self) -> usize {
        1
    }

    fn distribution(&self) -> Distribution {
        Distribution::Broadcast
    }

    fn encode(
        &self,
        _device: &Device,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        let (p, _) = GpioOutPayload::mut_from_prefix(&mut out[..]).unwrap();
        *p = GpioOutPayload {
            values: core::array::from_fn(|i| U64::new(self.outputs[i].encode())),
        };
        Ok(Cmd::SetGpioOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_device;

    #[test]
    fn gpio_out_encodes_tag_and_value() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetGpioOut {
            outputs: [
                GpioOut::Off,
                GpioOut::Direct(true),
                GpioOut::PwmOut(7),
                GpioOut::ModIdx(0x1234),
            ],
        }
        .encode(&test_device(0), 0, &mut out)
        .unwrap();
        assert_eq!(cmd, Cmd::SetGpioOut);
        assert_eq!(&out[0..8], &0u64.to_le_bytes());
        assert_eq!(
            &out[8..16],
            &((u64::from(GPIO_O_TYPE_DIRECT) << 56) | 1).to_le_bytes()
        );
        assert_eq!(
            &out[16..24],
            &((u64::from(GPIO_O_TYPE_PWM_OUT) << 56) | 7).to_le_bytes()
        );
        assert_eq!(
            &out[24..32],
            &((u64::from(GPIO_O_TYPE_MOD_IDX) << 56) | 0x1234).to_le_bytes()
        );
    }

    #[test]
    fn sys_time_eq_encodes_scaled_fpga_value() {
        let ec_time_ns = 0x0123_4567_89AB_CDEFu64;
        let expected = ((ec_time_ns / 3125) << 6) >> 9;
        let mut out = [0u8; PAYLOAD_BYTES];
        SetGpioOut {
            outputs: [
                GpioOut::Off,
                GpioOut::SysTimeEq(DcSysTime::from_nanos(ec_time_ns)),
                GpioOut::Off,
                GpioOut::Off,
            ],
        }
        .encode(&test_device(0), 0, &mut out)
        .unwrap();
        assert_eq!(
            &out[8..16],
            &((u64::from(GPIO_O_TYPE_SYS_TIME_EQ) << 56) | (expected & VALUE_MASK)).to_le_bytes()
        );
    }
}
