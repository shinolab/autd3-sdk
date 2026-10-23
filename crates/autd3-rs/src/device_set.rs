use std::ops::{Index, Sub};

use autd3_rs_core::MAX_DEVICES;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DeviceSet {
    bits: u128,
}

const _: () = assert!(MAX_DEVICES == u128::BITS as usize);

impl DeviceSet {
    #[must_use]
    pub(crate) const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[must_use]
    pub(crate) fn first_n(num_devices: usize) -> Self {
        Self {
            bits: u128::MAX
                .checked_shr(u32::try_from(MAX_DEVICES - num_devices).unwrap_or(u32::MAX))
                .unwrap_or(0),
        }
    }

    pub(crate) fn insert(&mut self, device: usize) {
        self.bits |= 1 << device;
    }

    #[must_use]
    pub(crate) fn contains(&self, device: usize) -> bool {
        u32::try_from(device)
            .ok()
            .and_then(|device| self.bits.checked_shr(device))
            .is_some_and(|shifted| shifted & 1 == 1)
    }

    #[must_use]
    pub(crate) const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    #[must_use]
    pub(crate) const fn is_subset(&self, other: &Self) -> bool {
        self.bits & other.bits == self.bits
    }

    #[must_use]
    pub(crate) const fn lowest(&self) -> Option<usize> {
        if self.is_empty() {
            None
        } else {
            Some(self.bits.trailing_zeros() as usize)
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..MAX_DEVICES).filter(|&device| self.contains(device))
    }
}

impl Index<usize> for DeviceSet {
    type Output = bool;

    fn index(&self, device: usize) -> &bool {
        if self.contains(device) { &true } else { &false }
    }
}

impl Sub for DeviceSet {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self {
        Self {
            bits: self.bits & !rhs.bits,
        }
    }
}

impl FromIterator<usize> for DeviceSet {
    fn from_iter<I: IntoIterator<Item = usize>>(devices: I) -> Self {
        let mut set = Self::empty();
        for device in devices {
            set.insert(device);
        }
        set
    }
}

impl std::fmt::Debug for DeviceSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceSet;
    use autd3_rs_core::MAX_DEVICES;

    #[test]
    fn indexing_tells_whether_a_device_is_in_the_set() {
        let set = DeviceSet::from_iter([1, 3]);
        assert!(!set[0]);
        assert!(set[1]);
        assert!(!set[2]);
        assert!(set[3]);
        assert!(!set[MAX_DEVICES]);
        assert!(!set[usize::MAX]);
    }

    #[test]
    fn first_n_holds_exactly_the_leading_devices() {
        assert!(DeviceSet::first_n(0).is_empty());
        assert_eq!(DeviceSet::first_n(3), DeviceSet::from_iter([0, 1, 2]));
        assert_eq!(DeviceSet::first_n(MAX_DEVICES).iter().count(), MAX_DEVICES);
        assert!(DeviceSet::first_n(MAX_DEVICES)[MAX_DEVICES - 1]);
    }

    #[test]
    fn difference_and_lowest_find_the_first_missing_device() {
        let missing = DeviceSet::first_n(4) - DeviceSet::from_iter([0, 3]);
        assert_eq!(missing, DeviceSet::from_iter([1, 2]));
        assert_eq!(missing.lowest(), Some(1));
        assert_eq!(DeviceSet::empty().lowest(), None);
    }

    #[test]
    fn is_subset_compares_membership() {
        let sent_to = DeviceSet::from_iter([0, 2]);
        assert!(sent_to.is_subset(&DeviceSet::from_iter([0, 1, 2])));
        assert!(!sent_to.is_subset(&DeviceSet::from_iter([0, 1])));
        assert!(DeviceSet::empty().is_subset(&DeviceSet::empty()));
    }

    #[test]
    fn debug_lists_the_devices() {
        assert_eq!(format!("{:?}", DeviceSet::from_iter([0, 5])), "{0, 5}");
    }
}
