use crate::error::Error;
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

const TYPE_NONE: u8 = 0x00;
const TYPE_BASE_SIG: u8 = 0x01;
const TYPE_THERMO: u8 = 0x02;
const TYPE_FORCE_FAN: u8 = 0x03;
const TYPE_SYNC: u8 = 0x10;
const TYPE_MOD_BANK: u8 = 0x20;
const TYPE_MOD_IDX: u8 = 0x21;
const TYPE_PATTERN_BANK: u8 = 0x50;
const TYPE_PATTERN_IDX: u8 = 0x51;
const TYPE_IS_PATTERN_MODE: u8 = 0x52;
const TYPE_SYS_TIME_EQ: u8 = 0x60;
const TYPE_SYNC_DIFF: u8 = 0x70;
const TYPE_PWM_OUT: u8 = 0xE0;
const TYPE_DIRECT: u8 = 0xF0;

const VALUE_MASK: u64 = 0x00FF_FFFF_FFFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GpioOut {
    #[default]
    None,
    BaseSignal,
    Thermo,
    ForceFan,
    Sync,
    ModBank,
    ModIdx(u16),
    PatternBank,
    PatternIdx(u16),
    IsPatternMode,
    SysTimeEq(u64),
    SyncDiff,
    PwmOut(u8),
    Direct(bool),
}

impl GpioOut {
    fn encode(self) -> u64 {
        let (tag, value): (u8, u64) = match self {
            GpioOut::None => (TYPE_NONE, 0),
            GpioOut::BaseSignal => (TYPE_BASE_SIG, 0),
            GpioOut::Thermo => (TYPE_THERMO, 0),
            GpioOut::ForceFan => (TYPE_FORCE_FAN, 0),
            GpioOut::Sync => (TYPE_SYNC, 0),
            GpioOut::ModBank => (TYPE_MOD_BANK, 0),
            GpioOut::ModIdx(idx) => (TYPE_MOD_IDX, u64::from(idx)),
            GpioOut::PatternBank => (TYPE_PATTERN_BANK, 0),
            GpioOut::PatternIdx(idx) => (TYPE_PATTERN_IDX, u64::from(idx)),
            GpioOut::IsPatternMode => (TYPE_IS_PATTERN_MODE, 0),
            GpioOut::SysTimeEq(t) => (TYPE_SYS_TIME_EQ, t),
            GpioOut::SyncDiff => (TYPE_SYNC_DIFF, 0),
            GpioOut::PwmOut(tr) => (TYPE_PWM_OUT, u64::from(tr)),
            GpioOut::Direct(on) => (TYPE_DIRECT, u64::from(on)),
        };
        (value & VALUE_MASK) | (u64::from(tag) << 56)
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
        _device: usize,
        _frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        for (i, output) in self.outputs.iter().enumerate() {
            out[8 * i..8 * i + 8].copy_from_slice(&output.encode().to_le_bytes());
        }
        Ok(Cmd::SetGpioOut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpio_out_encodes_tag_and_value() {
        let mut out = [0u8; PAYLOAD_BYTES];
        let cmd = SetGpioOut {
            outputs: [
                GpioOut::None,
                GpioOut::Direct(true),
                GpioOut::PwmOut(7),
                GpioOut::ModIdx(0x1234),
            ],
        }
        .encode(0, 0, &mut out)
        .unwrap();
        assert_eq!(cmd, Cmd::SetGpioOut);
        assert_eq!(&out[0..8], &0u64.to_le_bytes());
        assert_eq!(
            &out[8..16],
            &((u64::from(TYPE_DIRECT) << 56) | 1).to_le_bytes()
        );
        assert_eq!(
            &out[16..24],
            &((u64::from(TYPE_PWM_OUT) << 56) | 7).to_le_bytes()
        );
        assert_eq!(
            &out[24..32],
            &((u64::from(TYPE_MOD_IDX) << 56) | 0x1234).to_le_bytes()
        );
    }
}
