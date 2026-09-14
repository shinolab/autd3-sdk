use super::{Device, Geometry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TransducerMask<'a> {
    #[default]
    AllEnabled,
    Masked(&'a [Vec<bool>]),
    #[non_exhaustive]
    Group {
        indices: &'a [Vec<Option<usize>>],
        index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TransducerMaskError {
    #[error("the mask has {got} device slots but the geometry has {expected} devices")]
    DeviceCountMismatch { got: usize, expected: usize },
    #[error(
        "the mask slot for device {device} has {got} transducers but the device has {expected}"
    )]
    TransducerCountMismatch {
        device: usize,
        got: usize,
        expected: usize,
    },
}

fn validate_shape(
    geometry: &Geometry,
    num_devices: usize,
    num_transducers: impl Fn(usize) -> usize,
) -> Result<(), TransducerMaskError> {
    if num_devices != geometry.num_devices() {
        return Err(TransducerMaskError::DeviceCountMismatch {
            got: num_devices,
            expected: geometry.num_devices(),
        });
    }
    for (device, dev) in geometry.iter().enumerate() {
        let got = num_transducers(device);
        if got != dev.num_transducers() {
            return Err(TransducerMaskError::TransducerCountMismatch {
                device,
                got,
                expected: dev.num_transducers(),
            });
        }
    }
    Ok(())
}

impl TransducerMask<'_> {
    pub fn validate(self, geometry: &Geometry) -> Result<(), TransducerMaskError> {
        match self {
            Self::AllEnabled => Ok(()),
            Self::Masked(m) => validate_shape(geometry, m.len(), |device| m[device].len()),
            Self::Group { indices, .. } => {
                validate_shape(geometry, indices.len(), |device| indices[device].len())
            }
        }
    }

    #[must_use]
    pub fn is_enabled(self, device: usize, transducer: usize) -> bool {
        match self {
            Self::AllEnabled => true,
            Self::Masked(m) => m[device][transducer],
            Self::Group { indices, index } => indices[device][transducer] == Some(index),
        }
    }

    #[must_use]
    pub fn num_enabled(self, geometry: &Geometry) -> usize {
        match self {
            Self::AllEnabled => geometry.iter().map(Device::num_transducers).sum(),
            Self::Masked(m) => m.iter().flatten().filter(|&&b| b).count(),
            Self::Group { indices, index } => indices
                .iter()
                .flatten()
                .filter(|&&i| i == Some(index))
                .count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Autd3, TransducerGroups};
    use super::*;

    fn geometry() -> Geometry {
        Geometry::new(vec![Autd3::default(), Autd3::default()])
    }

    #[test]
    fn all_enabled_covers_every_transducer() {
        let geometry = geometry();
        let mask = TransducerMask::AllEnabled;
        assert_eq!(mask.validate(&geometry), Ok(()));
        assert!(mask.is_enabled(1, Autd3::NUM_TRANSDUCERS - 1));
        assert_eq!(mask.num_enabled(&geometry), 2 * Autd3::NUM_TRANSDUCERS);
    }

    #[test]
    fn masked_reads_the_flags() {
        let geometry = geometry();
        let mut flags = vec![vec![false; Autd3::NUM_TRANSDUCERS]; 2];
        flags[1][3] = true;
        flags[0][0] = true;
        let mask = TransducerMask::Masked(&flags);
        assert_eq!(mask.validate(&geometry), Ok(()));
        assert!(mask.is_enabled(1, 3));
        assert!(!mask.is_enabled(1, 0));
        assert_eq!(mask.num_enabled(&geometry), 2);
    }

    #[test]
    fn masked_shape_mismatch_is_an_error() {
        let geometry = geometry();
        let one_device = vec![vec![true; Autd3::NUM_TRANSDUCERS]];
        assert_eq!(
            TransducerMask::Masked(&one_device).validate(&geometry),
            Err(TransducerMaskError::DeviceCountMismatch {
                got: 1,
                expected: 2
            })
        );
        let short_row = vec![vec![true; Autd3::NUM_TRANSDUCERS], vec![true; 3]];
        assert_eq!(
            TransducerMask::Masked(&short_row).validate(&geometry),
            Err(TransducerMaskError::TransducerCountMismatch {
                device: 1,
                got: 3,
                expected: Autd3::NUM_TRANSDUCERS
            })
        );
    }

    #[test]
    fn group_mask_selects_the_key() {
        let geometry = geometry();
        let groups =
            TransducerGroups::new(&geometry, |device, tr| (tr < 10).then_some(device.idx()));
        let mask = groups.mask(1).unwrap();
        assert_eq!(mask.validate(&geometry), Ok(()));
        assert!(mask.is_enabled(1, 9));
        assert!(!mask.is_enabled(1, 10));
        assert!(!mask.is_enabled(0, 0));
        assert_eq!(mask.num_enabled(&geometry), 10);
    }

    #[test]
    fn group_mask_from_another_geometry_is_an_error() {
        let groups = TransducerGroups::new(&Geometry::new(vec![Autd3::default()]), |_, _| Some(0));
        assert_eq!(
            groups.mask(0).unwrap().validate(&geometry()),
            Err(TransducerMaskError::DeviceCountMismatch {
                got: 1,
                expected: 2
            })
        );
    }
}
