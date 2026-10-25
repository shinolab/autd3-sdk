use super::Geometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TransducerMask<'a> {
    #[default]
    AllEnabled,
    Masked(&'a [Vec<bool>]),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
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

impl TransducerMask<'_> {
    pub fn validate(self, geometry: &Geometry) -> Result<(), TransducerMaskError> {
        match self {
            Self::AllEnabled => Ok(()),
            Self::Masked(m) => {
                if m.len() != geometry.num_devices() {
                    return Err(TransducerMaskError::DeviceCountMismatch {
                        got: m.len(),
                        expected: geometry.num_devices(),
                    });
                }
                for (device, (row, dev)) in m.iter().zip(geometry.iter()).enumerate() {
                    if row.len() != dev.num_transducers() {
                        return Err(TransducerMaskError::TransducerCountMismatch {
                            device,
                            got: row.len(),
                            expected: dev.num_transducers(),
                        });
                    }
                }
                Ok(())
            }
        }
    }

    #[must_use]
    pub fn is_enabled(self, device: usize, transducer: usize) -> bool {
        match self {
            Self::AllEnabled => true,
            Self::Masked(m) => m[device][transducer],
        }
    }

    #[must_use]
    pub fn num_enabled(self, geometry: &Geometry) -> usize {
        match self {
            Self::AllEnabled => geometry.num_transducers(),
            Self::Masked(m) => m.iter().flatten().filter(|&&b| b).count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Autd3;
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
}
