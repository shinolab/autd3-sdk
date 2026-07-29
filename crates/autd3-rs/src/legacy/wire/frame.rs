use zerocopy::{FromBytes, Immutable, IntoBytes};

pub const TX_FRAME_BYTES: usize = 626;
pub const RX_FRAME_BYTES: usize = 2;
pub const HEADER_BYTES: usize = 4;
pub const PAYLOAD_BYTES: usize = TX_FRAME_BYTES - HEADER_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct MsgId(u8);

impl MsgId {
    pub const MAX: Self = Self(0x0F);

    #[must_use]
    pub const fn new(id: u8) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        if self.0 >= Self::MAX.0 {
            Self(0)
        } else {
            Self(self.0 + 1)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ack(u8);

impl Ack {
    #[must_use]
    pub const fn new(msg_id: u8, err: u8) -> Self {
        Self((err & 0x0F) << 4 | (msg_id & 0x0F))
    }

    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn msg_id(self) -> u8 {
        self.0 & 0x0F
    }

    #[must_use]
    pub const fn err(self) -> u8 {
        (self.0 >> 4) & 0x0F
    }
}

impl core::fmt::Debug for Ack {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ack")
            .field("msg_id", &self.msg_id())
            .field("err", &self.err())
            .finish()
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, FromBytes, IntoBytes, Immutable)]
pub struct Header {
    pub msg_id: u8,
    pad: u8,
    pub slot_2_offset: u16,
}

const _: () = assert!(size_of::<Header>() == HEADER_BYTES);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TxFrame {
    pub header: Header,
    pub payload: [u8; PAYLOAD_BYTES],
}

impl Default for TxFrame {
    fn default() -> Self {
        Self::new()
    }
}

impl TxFrame {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            header: Header {
                msg_id: 0,
                pad: 0,
                slot_2_offset: 0,
            },
            payload: [0; PAYLOAD_BYTES],
        }
    }

    pub fn write_to(&self, dst: &mut [u8; TX_FRAME_BYTES]) {
        dst[..HEADER_BYTES].copy_from_slice(self.header.as_bytes());
        dst[HEADER_BYTES..].copy_from_slice(&self.payload);
    }

    #[cfg(test)]
    #[must_use]
    pub fn to_bytes(&self) -> [u8; TX_FRAME_BYTES] {
        let mut bytes = [0u8; TX_FRAME_BYTES];
        self.write_to(&mut bytes);
        bytes
    }

    #[must_use]
    pub fn parse(src: &[u8; TX_FRAME_BYTES]) -> Self {
        let header = Header::read_from_bytes(&src[..HEADER_BYTES]).expect("header is 4 bytes");
        let mut payload = [0u8; PAYLOAD_BYTES];
        payload.copy_from_slice(&src[HEADER_BYTES..]);
        Self { header, payload }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxFrame {
    pub data: u8,
    pub ack: Ack,
}

impl RxFrame {
    #[must_use]
    pub const fn new(data: u8, ack: Ack) -> Self {
        Self { data, ack }
    }

    #[must_use]
    pub const fn parse(src: [u8; RX_FRAME_BYTES]) -> Self {
        Self {
            data: src[0],
            ack: Ack::from_bits(src[1]),
        }
    }

    pub const fn write_to(self, dst: &mut [u8; RX_FRAME_BYTES]) {
        dst[0] = self.data;
        dst[1] = self.ack.bits();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msg_id_wraps_at_0x0f() {
        assert_eq!(MsgId::new(0).next(), MsgId::new(1));
        assert_eq!(MsgId::new(0x0E).next(), MsgId::new(0x0F));
        assert_eq!(MsgId::new(0x0F).next(), MsgId::new(0));
    }

    #[test]
    fn ack_packs_err_high_msg_id_low() {
        let ack = Ack::new(0x05, 0x03);
        assert_eq!(ack.bits(), 0x35);
        assert_eq!(ack.msg_id(), 5);
        assert_eq!(ack.err(), 3);
        assert_eq!(format!("{ack:?}"), "Ack { msg_id: 5, err: 3 }");
    }

    #[test]
    fn tx_frame_round_trips_bytes() {
        let mut frame = TxFrame::new();
        frame.header.msg_id = 0x0A;
        frame.header.slot_2_offset = 0x0123;
        frame.payload[0] = 0x30;
        frame.payload[PAYLOAD_BYTES - 1] = 0xFF;

        let bytes = frame.to_bytes();
        assert_eq!(bytes[0], 0x0A);
        assert_eq!(bytes[1], 0x00);
        assert_eq!(&bytes[2..4], &0x0123u16.to_le_bytes());
        assert_eq!(bytes[4], 0x30);
        assert_eq!(bytes[TX_FRAME_BYTES - 1], 0xFF);
        assert_eq!(TxFrame::parse(&bytes), frame);
    }

    #[test]
    fn rx_frame_round_trips_bytes() {
        let rx = RxFrame::new(0xAB, Ack::new(3, 1));
        let mut bytes = [0u8; RX_FRAME_BYTES];
        rx.write_to(&mut bytes);
        assert_eq!(bytes, [0xAB, 0x13]);
        assert_eq!(RxFrame::parse(bytes), rx);
    }
}
