use autd3_rs_link_echocat::wire::{
    Command, DATAGRAM_HEADER_BYTES, ECAT_TYPE_DLPDU, ETH_HEADER_BYTES, ETHERTYPE_ETHERCAT,
    FRAME_HEADER_BYTES, LENGTH_MASK, LOCALLY_ADMINISTERED_BIT, MORE_DATAGRAMS, SOURCE_MAC_OFFSET,
    WKC_BYTES,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Outgoing,
    Response,
}

#[derive(Clone, Copy, Debug)]
pub struct Datagram<'a> {
    pub command: Command,
    pub index: u8,
    pub address: u32,
    pub data: &'a [u8],
    pub wkc: u16,
}

impl Datagram<'_> {
    #[must_use]
    pub const fn register(&self) -> u16 {
        let bytes = self.address.to_le_bytes();
        u16::from_le_bytes([bytes[2], bytes[3]])
    }
}

#[derive(Clone, Debug)]
pub struct EtherCatFrame<'a> {
    pub index: u8,
    pub direction: Direction,
    pub datagrams: Vec<Datagram<'a>>,
}

fn walk(buf: &[u8]) -> Option<Vec<Datagram<'_>>> {
    let mut datagrams = Vec::new();
    let mut at = FRAME_HEADER_BYTES;
    loop {
        if at + DATAGRAM_HEADER_BYTES + WKC_BYTES > buf.len() {
            return None;
        }
        let command = Command::from_code(buf[at])?;
        let index = buf[at + 1];
        let address = u32::from_le_bytes([buf[at + 2], buf[at + 3], buf[at + 4], buf[at + 5]]);
        let length_field = u16::from_le_bytes([buf[at + 6], buf[at + 7]]);
        let len = usize::from(length_field & LENGTH_MASK);
        let data_at = at + DATAGRAM_HEADER_BYTES;
        if data_at + len + WKC_BYTES > buf.len() {
            return None;
        }
        let wkc = u16::from_le_bytes([buf[data_at + len], buf[data_at + len + 1]]);
        datagrams.push(Datagram {
            command,
            index,
            address,
            data: &buf[data_at..data_at + len],
            wkc,
        });
        at = data_at + len + WKC_BYTES;
        if length_field & MORE_DATAGRAMS == 0 {
            return Some(datagrams);
        }
    }
}

#[must_use]
pub fn parse(buf: &[u8]) -> Option<EtherCatFrame<'_>> {
    if buf.len() < FRAME_HEADER_BYTES + DATAGRAM_HEADER_BYTES + WKC_BYTES {
        return None;
    }
    if u16::from_be_bytes([buf[12], buf[13]]) != ETHERTYPE_ETHERCAT {
        return None;
    }
    let header = u16::from_le_bytes([buf[ETH_HEADER_BYTES], buf[ETH_HEADER_BYTES + 1]]);
    if header >> 12 != ECAT_TYPE_DLPDU {
        return None;
    }
    let declared = usize::from(header & LENGTH_MASK);
    if FRAME_HEADER_BYTES + declared > buf.len() {
        return None;
    }
    let datagrams = walk(&buf[..FRAME_HEADER_BYTES + declared])?;
    let locally_administered = buf[SOURCE_MAC_OFFSET] & LOCALLY_ADMINISTERED_BIT != 0;
    let acknowledged = datagrams.iter().any(|datagram| datagram.wkc != 0);
    let direction = if locally_administered || acknowledged {
        Direction::Response
    } else {
        Direction::Outgoing
    };
    Some(EtherCatFrame {
        index: buf[FRAME_HEADER_BYTES + 1],
        direction,
        datagrams,
    })
}
