use super::{Device, Geometry, TransducerMask};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransducerGroups<K> {
    keys: Vec<K>,
    indices: Vec<Vec<Option<usize>>>,
}

impl<K: Copy + Eq> TransducerGroups<K> {
    #[must_use]
    pub fn new<F>(geometry: &Geometry, mut key: F) -> Self
    where
        F: FnMut(&Device, usize) -> Option<K>,
    {
        let mut keys = Vec::new();
        let indices = geometry
            .iter()
            .map(|device| {
                (0..device.num_transducers())
                    .map(|tr| {
                        key(device, tr).map(|k| {
                            keys.iter()
                                .position(|&known| known == k)
                                .unwrap_or_else(|| {
                                    keys.push(k);
                                    keys.len() - 1
                                })
                        })
                    })
                    .collect()
            })
            .collect();
        Self { keys, indices }
    }

    #[must_use]
    pub fn keys(&self) -> &[K] {
        &self.keys
    }

    #[must_use]
    pub fn key(&self, device: usize, transducer: usize) -> Option<K> {
        self.index(device, transducer).map(|index| self.keys[index])
    }

    #[must_use]
    pub fn index(&self, device: usize, transducer: usize) -> Option<usize> {
        self.indices[device][transducer]
    }

    #[must_use]
    pub fn indices(&self, device: usize) -> &[Option<usize>] {
        &self.indices[device]
    }

    #[must_use]
    pub fn num_devices(&self) -> usize {
        self.indices.len()
    }

    #[must_use]
    pub fn num_transducers(&self, device: usize) -> usize {
        self.indices[device].len()
    }

    #[must_use]
    pub fn num_transducers_in(&self, key: K) -> usize {
        self.position(key).map_or(0, |index| {
            self.indices
                .iter()
                .flatten()
                .filter(|&&i| i == Some(index))
                .count()
        })
    }

    #[must_use]
    pub fn mask(&self, key: K) -> Option<TransducerMask<'_>> {
        self.position(key).map(|index| TransducerMask::Group {
            indices: &self.indices,
            index,
        })
    }

    fn position(&self, key: K) -> Option<usize> {
        self.keys.iter().position(|&k| k == key)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Autd3;
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Side {
        Left,
        Right,
    }

    #[test]
    fn keys_are_recorded_in_first_appearance_order() {
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let groups = TransducerGroups::new(&geometry, |device, tr| match (device.idx(), tr % 3) {
            (_, 0) => None,
            (0, _) => Some(Side::Right),
            _ => Some(Side::Left),
        });

        assert_eq!(groups.keys(), &[Side::Right, Side::Left]);
        assert_eq!(groups.num_devices(), 2);
        assert_eq!(groups.num_transducers(1), Autd3::NUM_TRANSDUCERS);
        assert_eq!(groups.key(0, 0), None);
        assert_eq!(groups.key(0, 1), Some(Side::Right));
        assert_eq!(groups.key(1, 1), Some(Side::Left));
        assert_eq!(groups.index(1, 1), Some(1));
        assert_eq!(groups.indices(1)[1], Some(1));
        assert_eq!(groups.indices(0).len(), Autd3::NUM_TRANSDUCERS);
        assert_eq!(
            groups.num_transducers_in(Side::Right),
            (0..Autd3::NUM_TRANSDUCERS).filter(|tr| tr % 3 != 0).count()
        );
    }

    #[test]
    fn a_key_without_transducers_has_no_mask() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let groups = TransducerGroups::new(&geometry, |_, _| Some(Side::Left));
        assert!(groups.mask(Side::Left).is_some());
        assert!(groups.mask(Side::Right).is_none());
        assert_eq!(groups.num_transducers_in(Side::Right), 0);

        let empty = TransducerGroups::<Side>::new(&geometry, |_, _| None);
        assert!(empty.keys().is_empty());
    }
}
