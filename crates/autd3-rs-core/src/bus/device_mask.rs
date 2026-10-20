use std::ops::{BitOr, BitOrAssign, Index};

use crate::protocol::MAX_DEVICES;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct DeviceMask(u128);

impl DeviceMask {
    #[must_use]
    pub const fn from_bits(bits: u128) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u128 {
        self.0
    }

    #[must_use]
    pub const fn contains(self, device: usize) -> bool {
        device < MAX_DEVICES && self.0 >> device & 1 == 1
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    #[must_use]
    pub const fn iter(self) -> DeviceMaskIter {
        DeviceMaskIter(self.0)
    }
}

impl Index<usize> for DeviceMask {
    type Output = bool;

    fn index(&self, device: usize) -> &bool {
        if self.contains(device) { &true } else { &false }
    }
}

impl BitOr for DeviceMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for DeviceMask {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl FromIterator<usize> for DeviceMask {
    fn from_iter<I: IntoIterator<Item = usize>>(devices: I) -> Self {
        Self(
            devices
                .into_iter()
                .filter(|&device| device < MAX_DEVICES)
                .fold(0, |bits, device| bits | 1 << device),
        )
    }
}

impl IntoIterator for DeviceMask {
    type Item = usize;
    type IntoIter = DeviceMaskIter;

    fn into_iter(self) -> DeviceMaskIter {
        self.iter()
    }
}

#[derive(Debug, Clone)]
pub struct DeviceMaskIter(u128);

impl Iterator for DeviceMaskIter {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.0 == 0 {
            return None;
        }
        let device = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(device)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.count_ones() as usize;
        (len, Some(len))
    }
}

impl ExactSizeIterator for DeviceMaskIter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexing_tells_whether_a_device_is_in_the_mask() {
        let mask = DeviceMask::from_bits(0b1010);
        assert!(!mask[0]);
        assert!(mask[1]);
        assert!(!mask[2]);
        assert!(mask[3]);
        assert!(!mask[MAX_DEVICES]);
        assert!(!mask[usize::MAX]);
    }

    #[test]
    fn the_last_device_is_reachable() {
        let mask = DeviceMask::from_bits(1 << (MAX_DEVICES - 1));
        assert!(mask[MAX_DEVICES - 1]);
        assert_eq!(mask.iter().collect::<Vec<_>>(), [MAX_DEVICES - 1]);
    }

    #[test]
    fn the_devices_are_listed_in_ascending_order() {
        let mask = DeviceMask::from_bits(0b1_0110);
        assert_eq!(mask.iter().collect::<Vec<_>>(), [1, 2, 4]);
        assert_eq!(mask.into_iter().collect::<Vec<_>>(), [1, 2, 4]);
        assert_eq!(mask.len(), 3);
        assert_eq!(mask.bits(), 0b1_0110);
    }

    #[test]
    fn masks_are_joined_and_collected_from_device_indices() {
        let mut mask = DeviceMask::from_bits(0b001) | DeviceMask::from_bits(0b100);
        assert_eq!(mask.bits(), 0b101);
        mask |= DeviceMask::from_bits(0b010);
        assert_eq!(mask.bits(), 0b111);
        assert_eq!(
            [3, 0, 3, MAX_DEVICES].into_iter().collect::<DeviceMask>(),
            DeviceMask::from_bits(0b1001)
        );
    }

    #[test]
    fn the_default_is_empty() {
        let mask = DeviceMask::default();
        assert!(mask.is_empty());
        assert_eq!(mask.len(), 0);
        assert_eq!(mask.iter().count(), 0);
        assert!(!DeviceMask::from_bits(1).is_empty());
    }
}
