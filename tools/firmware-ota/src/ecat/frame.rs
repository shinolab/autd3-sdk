pub const ETHERTYPE_ETHERCAT: u16 = 0x88a4;
pub const ETH_HEADER_BYTES: usize = 14;
pub const ECAT_HEADER_BYTES: usize = 2;
pub const FRAME_HEADER_BYTES: usize = ETH_HEADER_BYTES + ECAT_HEADER_BYTES;
pub const DATAGRAM_HEADER_BYTES: usize = 10;
pub const WKC_BYTES: usize = 2;
pub const DATAGRAM_OVERHEAD_BYTES: usize = DATAGRAM_HEADER_BYTES + WKC_BYTES;
pub const MIN_ETHERNET_FRAME_BYTES: usize = 60;
pub const MAX_DATAGRAM_DATA_BYTES: usize = 0x07ff;

const BROADCAST_MAC: [u8; 6] = [0xff; 6];
const MASTER_MAC: [u8; 6] = [0x10; 6];
const ECAT_TYPE_DLPDU: u16 = 1;
const LENGTH_MASK: u16 = 0x07ff;
pub const MORE_DATAGRAMS: u16 = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Command {
    Nop = 0x00,
    Apwr = 0x02,
    Fprd = 0x04,
    Fpwr = 0x05,
    Brd = 0x07,
    Bwr = 0x08,
    Lrd = 0x0a,
    Lwr = 0x0b,
    Frmw = 0x0e,
}

impl Command {
    #[must_use]
    pub const fn code(self) -> u8 {
        self as u8
    }

    #[cfg(test)]
    #[must_use]
    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0x00 => Some(Self::Nop),
            0x02 => Some(Self::Apwr),
            0x04 => Some(Self::Fprd),
            0x05 => Some(Self::Fpwr),
            0x07 => Some(Self::Brd),
            0x08 => Some(Self::Bwr),
            0x0a => Some(Self::Lrd),
            0x0b => Some(Self::Lwr),
            0x0e => Some(Self::Frmw),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Address {
    Position { adp: u16, register: u16 },
    Node { node: u16, register: u16 },
    Broadcast { register: u16 },
    Logical(u32),
}

impl Address {
    #[must_use]
    pub const fn position(position: u16, register: u16) -> Self {
        Self::Position {
            adp: 0u16.wrapping_sub(position),
            register,
        }
    }

    #[must_use]
    pub const fn node(node: u16, register: u16) -> Self {
        Self::Node { node, register }
    }

    #[must_use]
    pub const fn broadcast(register: u16) -> Self {
        Self::Broadcast { register }
    }

    fn encode(self) -> [u8; 4] {
        let (low, high) = match self {
            Self::Position { adp, register } => (adp, register),
            Self::Node { node, register } => (node, register),
            Self::Broadcast { register } => (0, register),
            Self::Logical(address) => return address.to_le_bytes(),
        };
        let [l0, l1] = low.to_le_bytes();
        let [h0, h1] = high.to_le_bytes();
        [l0, l1, h0, h1]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("datagram does not fit in the frame buffer")]
    FrameFull,
    #[error("frame is shorter than an EtherCAT header")]
    Truncated,
    #[error("ethertype {0:#06x} is not EtherCAT")]
    NotEtherCat(u16),
    #[error("EtherCAT frame type {0} is not a DLPDU")]
    NotDlpdu(u16),
    #[error("declared datagram length {declared} exceeds the {received} bytes received")]
    LengthMismatch { declared: usize, received: usize },
    #[error("frame index {received} does not match the {expected} that was sent")]
    IndexMismatch { expected: u8, received: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    data: usize,
    len: usize,
}

impl Slot {
    const fn end(self) -> usize {
        self.data + self.len + WKC_BYTES
    }
}

#[must_use]
pub fn frame_bytes_for(data_len: usize) -> usize {
    (FRAME_HEADER_BYTES + DATAGRAM_OVERHEAD_BYTES + data_len).max(MIN_ETHERNET_FRAME_BYTES)
}

pub struct FrameBuilder<'a> {
    buf: &'a mut [u8],
    index: u8,
    len: usize,
    last_length_field: Option<usize>,
}

impl<'a> FrameBuilder<'a> {
    pub fn new(buf: &'a mut [u8], index: u8) -> Self {
        assert!(
            buf.len() >= MIN_ETHERNET_FRAME_BYTES,
            "frame buffer must hold a minimum-size ethernet frame"
        );
        buf[..6].copy_from_slice(&BROADCAST_MAC);
        buf[6..12].copy_from_slice(&MASTER_MAC);
        buf[12..14].copy_from_slice(&ETHERTYPE_ETHERCAT.to_be_bytes());
        Self {
            buf,
            index,
            len: FRAME_HEADER_BYTES,
            last_length_field: None,
        }
    }

    pub fn push(
        &mut self,
        command: Command,
        address: Address,
        data_len: usize,
    ) -> Result<Slot, FrameError> {
        if data_len > MAX_DATAGRAM_DATA_BYTES
            || self.len + DATAGRAM_OVERHEAD_BYTES + data_len > self.buf.len()
        {
            return Err(FrameError::FrameFull);
        }
        if let Some(at) = self.last_length_field {
            let flagged = u16::from_le_bytes([self.buf[at], self.buf[at + 1]]) | MORE_DATAGRAMS;
            self.buf[at..at + 2].copy_from_slice(&flagged.to_le_bytes());
        }

        let at = self.len;
        self.buf[at] = command.code();
        self.buf[at + 1] = self.index;
        self.buf[at + 2..at + 6].copy_from_slice(&address.encode());
        let length_field = u16::try_from(data_len).expect("bounded by the length mask");
        self.buf[at + 6..at + 8].copy_from_slice(&length_field.to_le_bytes());
        self.buf[at + 8..at + 10].fill(0);
        self.buf[at + DATAGRAM_HEADER_BYTES..at + DATAGRAM_OVERHEAD_BYTES + data_len].fill(0);

        self.last_length_field = Some(at + 6);
        self.len += DATAGRAM_OVERHEAD_BYTES + data_len;
        Ok(Slot {
            data: at + DATAGRAM_HEADER_BYTES,
            len: data_len,
        })
    }

    pub fn data_mut(&mut self, slot: Slot) -> &mut [u8] {
        &mut self.buf[slot.data..slot.data + slot.len]
    }

    #[must_use]
    pub fn finish(self) -> usize {
        let datagram_bytes = self.len - FRAME_HEADER_BYTES;
        let header = u16::try_from(datagram_bytes).expect("bounded by the frame buffer")
            & LENGTH_MASK
            | (ECAT_TYPE_DLPDU << 12);
        self.buf[ETH_HEADER_BYTES..FRAME_HEADER_BYTES].copy_from_slice(&header.to_le_bytes());
        let padded = self.len.max(MIN_ETHERNET_FRAME_BYTES);
        self.buf[self.len..padded].fill(0);
        padded
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    buf: &'a [u8],
}

impl<'a> FrameView<'a> {
    pub fn parse(buf: &'a [u8], expected_index: u8) -> Result<Self, FrameError> {
        if buf.len() < FRAME_HEADER_BYTES + DATAGRAM_OVERHEAD_BYTES {
            return Err(FrameError::Truncated);
        }
        let ethertype = u16::from_be_bytes([buf[12], buf[13]]);
        if ethertype != ETHERTYPE_ETHERCAT {
            return Err(FrameError::NotEtherCat(ethertype));
        }
        let header = u16::from_le_bytes([buf[ETH_HEADER_BYTES], buf[ETH_HEADER_BYTES + 1]]);
        let frame_type = header >> 12;
        if frame_type != ECAT_TYPE_DLPDU {
            return Err(FrameError::NotDlpdu(frame_type));
        }
        let declared = usize::from(header & LENGTH_MASK);
        if FRAME_HEADER_BYTES + declared > buf.len() {
            return Err(FrameError::LengthMismatch {
                declared,
                received: buf.len(),
            });
        }
        let index = buf[FRAME_HEADER_BYTES + 1];
        if index != expected_index {
            return Err(FrameError::IndexMismatch {
                expected: expected_index,
                received: index,
            });
        }
        Ok(Self { buf })
    }

    pub fn data(&self, slot: Slot) -> Result<&'a [u8], FrameError> {
        if slot.end() > self.buf.len() {
            return Err(FrameError::Truncated);
        }
        Ok(&self.buf[slot.data..slot.data + slot.len])
    }

    pub fn wkc(&self, slot: Slot) -> Result<u16, FrameError> {
        if slot.end() > self.buf.len() {
            return Err(FrameError::Truncated);
        }
        let at = slot.data + slot.len;
        Ok(u16::from_le_bytes([self.buf[at], self.buf[at + 1]]))
    }
}

#[must_use]
pub fn frame_index(frame: &[u8]) -> Option<u8> {
    frame.get(FRAME_HEADER_BYTES + 1).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_datagram_frame_is_padded_to_the_ethernet_minimum() {
        let mut buf = [0u8; 128];
        let mut builder = FrameBuilder::new(&mut buf, 0x42);
        builder
            .push(Command::Brd, Address::broadcast(0x0130), 2)
            .unwrap();
        let len = builder.finish();

        assert_eq!(len, MIN_ETHERNET_FRAME_BYTES);
        assert_eq!(len, frame_bytes_for(2));
        assert_eq!(&buf[..6], &[0xff; 6]);
        assert_eq!(u16::from_be_bytes([buf[12], buf[13]]), ETHERTYPE_ETHERCAT);
        let header = u16::from_le_bytes([buf[ETH_HEADER_BYTES], buf[ETH_HEADER_BYTES + 1]]);
        assert_eq!(header >> 12, 1);
        assert_eq!(usize::from(header & 0x07ff), DATAGRAM_OVERHEAD_BYTES + 2);
        assert_eq!(buf[FRAME_HEADER_BYTES], Command::Brd.code());
        assert_eq!(frame_index(&buf), Some(0x42));
    }

    #[test]
    fn every_datagram_but_the_last_sets_the_more_flag() {
        let mut buf = [0u8; 256];
        let mut builder = FrameBuilder::new(&mut buf, 1);
        let slots = [
            builder.push(Command::Lwr, Address::Logical(0), 8).unwrap(),
            builder
                .push(Command::Frmw, Address::node(0x1000, 0x0910), 8)
                .unwrap(),
            builder
                .push(Command::Brd, Address::broadcast(0x0130), 2)
                .unwrap(),
        ];
        let _ = builder.finish();

        let more = |slot: Slot| {
            let at = slot.data - 4;
            u16::from_le_bytes([buf[at], buf[at + 1]]) & MORE_DATAGRAMS != 0
        };
        assert_eq!(slots.map(more), [true, true, false]);
    }

    #[test]
    fn slots_address_the_same_bytes_in_the_response() {
        let mut buf = [0u8; 256];
        let mut builder = FrameBuilder::new(&mut buf, 7);
        let slot = builder
            .push(Command::Fprd, Address::node(0x1000, 0x0130), 2)
            .unwrap();
        let len = builder.finish();

        buf[slot.data..slot.data + 2].copy_from_slice(&[0x08, 0x00]);
        buf[slot.data + 2..slot.data + 4].copy_from_slice(&1u16.to_le_bytes());

        let view = FrameView::parse(&buf[..len], 7).unwrap();
        assert_eq!(view.data(slot).unwrap(), &[0x08, 0x00]);
        assert_eq!(view.wkc(slot).unwrap(), 1);
    }

    #[test]
    fn addresses_encode_adp_then_ado_little_endian() {
        assert_eq!(
            Address::node(0x1001, 0x0920).encode(),
            [0x01, 0x10, 0x20, 0x09]
        );
        assert_eq!(
            Address::position(1, 0x0130).encode(),
            [0xff, 0xff, 0x30, 0x01]
        );
        assert_eq!(
            Address::Logical(0x1234_5678).encode(),
            [0x78, 0x56, 0x34, 0x12]
        );
    }

    #[test]
    fn parse_rejects_foreign_and_mismatched_frames() {
        let mut buf = [0u8; 128];
        let mut builder = FrameBuilder::new(&mut buf, 3);
        builder
            .push(Command::Brd, Address::broadcast(0x0130), 2)
            .unwrap();
        let len = builder.finish();

        assert_eq!(
            FrameView::parse(&buf[..len], 4).unwrap_err(),
            FrameError::IndexMismatch {
                expected: 4,
                received: 3
            }
        );
        assert_eq!(
            FrameView::parse(&buf[..20], 3).unwrap_err(),
            FrameError::Truncated
        );
        let mut foreign = buf;
        foreign[12..14].copy_from_slice(&0x0800u16.to_be_bytes());
        assert_eq!(
            FrameView::parse(&foreign[..len], 3).unwrap_err(),
            FrameError::NotEtherCat(0x0800)
        );
    }

    #[test]
    fn push_rejects_datagrams_that_do_not_fit() {
        let mut buf = [0u8; 64];
        let mut builder = FrameBuilder::new(&mut buf, 0);
        assert_eq!(
            builder.push(Command::Lwr, Address::Logical(0), 64),
            Err(FrameError::FrameFull)
        );
    }
}
