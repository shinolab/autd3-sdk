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

    pub(crate) fn write_to(self, dst: &mut [u8; 4]) {
        let (low, high) = match self {
            Self::Position { adp, register } => (adp, register),
            Self::Node { node, register } => (node, register),
            Self::Broadcast { register } => (0, register),
            Self::Logical(address) => {
                *dst = address.to_le_bytes();
                return;
            }
        };
        dst[..2].copy_from_slice(&low.to_le_bytes());
        dst[2..].copy_from_slice(&high.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::Address;

    #[test]
    fn auto_increment_addresses_count_down_from_zero() {
        assert_eq!(
            Address::position(0, 0x0130),
            Address::Position {
                adp: 0,
                register: 0x0130
            }
        );
        assert_eq!(
            Address::position(1, 0x0130),
            Address::Position {
                adp: 0xffff,
                register: 0x0130
            }
        );
        assert_eq!(
            Address::position(3, 0x0130),
            Address::Position {
                adp: 0xfffd,
                register: 0x0130
            }
        );
    }

    #[test]
    fn physical_addresses_encode_adp_then_ado_little_endian() {
        let mut dst = [0u8; 4];
        Address::node(0x1001, 0x0920).write_to(&mut dst);
        assert_eq!(dst, [0x01, 0x10, 0x20, 0x09]);
    }

    #[test]
    fn logical_addresses_occupy_the_whole_field() {
        let mut dst = [0u8; 4];
        Address::Logical(0x1234_5678).write_to(&mut dst);
        assert_eq!(dst, [0x78, 0x56, 0x34, 0x12]);
    }
}
