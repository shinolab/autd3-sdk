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

    pub(crate) fn stage(&mut self, seq: Seq, cmd: Cmd, payload: &[u8]) {
        let (header, body) = FrameHeader::mut_from_prefix(self.bytes.as_mut_slice()).unwrap();
        *header = FrameHeader {
            seq: seq.get(),
            cmd: cmd.as_u8(),
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
