use autd3_rs_core::FRAME_BYTES_MAX;
use std::time::Duration;

use autd3_rs_core::protocol::{Cmd, FrameHeader, Seq};
use zerocopy::FromBytes;

use crate::error::Error;
use crate::value::SysTime;

const NS_PER_MS: u64 = 1_000_000;

pub(crate) fn schedule(
    lead: Option<Duration>,
    now: impl FnOnce() -> Option<SysTime>,
) -> Result<u32, Error> {
    let Some(lead) = lead else {
        return Ok(0);
    };
    let now = now().ok_or(Error::DeviceTimeUnknown)?;
    Ok(exec_at(now, lead))
}

#[must_use]
fn exec_at(now: SysTime, lead: Duration) -> u32 {
    let ms = (now + lead).sys_time().div_ceil(NS_PER_MS);
    match u32::try_from(ms & u64::from(u32::MAX)) {
        Ok(0) | Err(_) => 1,
        Ok(ms) => ms,
    }
}

#[derive(Clone)]
pub(crate) struct FrameBuf {
    bytes: [u8; FRAME_BYTES_MAX],
    len: usize,
}

impl FrameBuf {
    pub(crate) const fn new() -> Self {
        Self {
            bytes: [0; FRAME_BYTES_MAX],
            len: size_of::<FrameHeader>(),
        }
    }

    pub(crate) fn stage(&mut self, seq: Seq, cmd: Cmd, exec_at: u32, payload: &[u8]) {
        let (header, body) = FrameHeader::mut_from_prefix(self.bytes.as_mut_slice()).unwrap();
        *header = FrameHeader {
            seq: seq.get(),
            cmd: cmd.as_u8(),
            exec_at: exec_at.into(),
        };
        body[..payload.len()].copy_from_slice(payload);
        self.len = size_of::<FrameHeader>() + payload.len();
    }
}

impl AsRef<[u8]> for FrameBuf {
    fn as_ref(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = NS_PER_MS;

    #[test]
    fn exec_at_rounds_up_to_the_next_millisecond_boundary() {
        let lead = Duration::from_millis(3);
        assert_eq!(exec_at(SysTime::from_nanos(10 * MS), lead), 13);
        assert_eq!(exec_at(SysTime::from_nanos(10 * MS + 1), lead), 14);
        assert_eq!(exec_at(SysTime::from_nanos(11 * MS - 1), lead), 14);
    }

    #[test]
    fn exec_at_keeps_the_low_32_bits_and_never_yields_zero() {
        let lead = Duration::from_millis(1);
        let wrap = u64::from(u32::MAX) + 1;
        assert_eq!(exec_at(SysTime::from_nanos((wrap - 1) * MS), lead), 1);
        assert_eq!(exec_at(SysTime::from_nanos(wrap * MS), lead), 1);
        assert_eq!(exec_at(SysTime::from_nanos((wrap + 1) * MS), lead), 2);
    }

    #[test]
    fn without_a_lead_the_frame_is_immediate_and_the_time_is_not_read() {
        assert_eq!(
            schedule(None, || unreachable!("the device time is read")).unwrap(),
            0
        );
    }

    #[test]
    fn with_a_lead_the_frame_is_scheduled_from_the_device_time() {
        assert_eq!(
            schedule(Some(Duration::from_millis(5)), || Some(
                SysTime::from_nanos(20 * MS)
            ))
            .unwrap(),
            25
        );
    }

    #[test]
    fn with_a_lead_an_unknown_device_time_fails() {
        assert!(matches!(
            schedule(Some(Duration::from_millis(5)), || None),
            Err(Error::DeviceTimeUnknown)
        ));
    }

    #[test]
    fn stage_writes_exec_at_into_the_header() {
        let mut buf = FrameBuf::new();
        buf.stage(Seq::ZERO, Cmd::Nop, 0x0403_0201, &[0xAA]);
        assert_eq!(
            buf.as_ref(),
            &[0, Cmd::Nop.as_u8(), 0x01, 0x02, 0x03, 0x04, 0xAA]
        );
    }
}
