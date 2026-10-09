use super::{Device, Geometry, TransducerMask};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransducerGroups<K> {
    keys: Vec<K>,
    indices: Vec<Vec<usize>>,
}

impl<K: Copy + Eq> TransducerGroups<K> {
    #[must_use]
    pub fn new<F>(geometry: &Geometry, mut key: F) -> Self
    where
        F: FnMut(&Device, usize) -> K,
    {
        let mut keys = Vec::new();
        let indices = geometry
            .iter()
            .map(|device| {
                (0..device.num_transducers())
                    .map(|tr| {
                        let k = key(device, tr);
                        keys.iter()
                            .position(|&known| known == k)
                            .unwrap_or_else(|| {
                                keys.push(k);
                                keys.len() - 1
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
    pub fn key(&self, device: usize, transducer: usize) -> K {
        self.keys[self.index(device, transducer)]
    }

    #[must_use]
    pub fn index(&self, device: usize, transducer: usize) -> usize {
        self.indices[device][transducer]
    }

    #[must_use]
    pub fn indices(&self, device: usize) -> &[usize] {
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
                .filter(|&&i| i == index)
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

    pub fn masks(&self) -> impl Iterator<Item = (K, TransducerMask<'_>)> {
        self.keys.iter().enumerate().map(|(index, &key)| {
            (
                key,
                TransducerMask::Group {
                    indices: &self.indices,
                    index,
                },
            )
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

    fn sides(device: &Device, tr: usize) -> Side {
        if device.idx() == 0 && !tr.is_multiple_of(3) {
            Side::Right
        } else {
            Side::Left
        }
    }

    #[test]
    fn keys_are_recorded_in_first_appearance_order() {
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let groups = TransducerGroups::new(&geometry, |device, tr| match (device.idx(), tr) {
            (0, 0) => Side::Right,
            _ => sides(device, tr),
        });

        assert_eq!(groups.keys(), &[Side::Right, Side::Left]);
        assert_eq!(groups.num_devices(), 2);
        assert_eq!(groups.num_transducers(1), Autd3::NUM_TRANSDUCERS);
        assert_eq!(groups.key(0, 1), Side::Right);
        assert_eq!(groups.key(0, 3), Side::Left);
        assert_eq!(groups.key(1, 1), Side::Left);
        assert_eq!(groups.index(1, 1), 1);
        assert_eq!(groups.indices(1)[1], 1);
        assert_eq!(groups.indices(0).len(), Autd3::NUM_TRANSDUCERS);
        assert_eq!(
            groups.num_transducers_in(Side::Right),
            1 + (1..Autd3::NUM_TRANSDUCERS)
                .filter(|tr| !tr.is_multiple_of(3))
                .count()
        );
    }

    #[test]
    fn a_key_without_transducers_has_no_mask() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let groups = TransducerGroups::new(&geometry, |_, _| Side::Left);
        assert!(groups.mask(Side::Left).is_some());
        assert!(groups.mask(Side::Right).is_none());
        assert_eq!(groups.num_transducers_in(Side::Right), 0);
    }

    #[test]
    fn masks_lists_every_key_in_first_appearance_order() {
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let groups = TransducerGroups::new(&geometry, sides);

        let masks: Vec<_> = groups.masks().collect();
        assert_eq!(masks.len(), 2);
        for (&key, (listed, mask)) in groups.keys().iter().zip(masks) {
            assert_eq!(listed, key);
            assert_eq!(Some(mask), groups.mask(key));
        }
    }
}
