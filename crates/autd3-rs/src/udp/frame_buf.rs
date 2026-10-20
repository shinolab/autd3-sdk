use autd3_rs_core::FRAME_BYTES_MAX;

use autd3_rs_core::protocol::{Cmd, FrameHeader, Seq};
use zerocopy::FromBytes;

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

    pub(crate) fn write_frame(&mut self, seq: Seq, cmd: Cmd, exec_at: u32, payload: &[u8]) {
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

    #[test]
    fn write_frame_puts_exec_at_into_the_header() {
        let mut buf = FrameBuf::new();
        buf.write_frame(Seq::ZERO, Cmd::Nop, 0x0403_0201, &[0xAA]);
        assert_eq!(
            buf.as_ref(),
            &[0, Cmd::Nop.as_u8(), 0x01, 0x02, 0x03, 0x04, 0xAA]
        );
    }
}
