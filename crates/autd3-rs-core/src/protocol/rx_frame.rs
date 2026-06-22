use super::{RX_FRAME_BYTES, Seq};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxFrame {
    pub ack: Seq,
    pub data: u8,
}

const _: () = assert!(size_of::<RxFrame>() == RX_FRAME_BYTES);

impl RxFrame {
    #[must_use]
    pub const fn parse(src: &[u8; RX_FRAME_BYTES]) -> Self {
        Self {
            ack: Seq::new(src[0]),
            data: src[1],
        }
    }

    pub fn write_to(self, dst: &mut [u8; RX_FRAME_BYTES]) {
        dst[0] = self.ack.get();
        dst[1] = self.data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rx_frame_round_trips() {
        let f = RxFrame {
            ack: Seq::new(0x42),
            data: 0x80,
        };
        let mut bytes = [0u8; RX_FRAME_BYTES];
        f.write_to(&mut bytes);
        assert_eq!(bytes, [0x42, 0x80]);
        assert_eq!(RxFrame::parse(&bytes), f);
    }
}
