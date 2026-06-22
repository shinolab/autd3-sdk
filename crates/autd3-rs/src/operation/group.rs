use std::collections::HashMap;
use std::hash::Hash;

use crate::error::{Error, PayloadError};
use crate::geometry::{Device, Geometry};
use crate::protocol::{Cmd, PAYLOAD_BYTES};

use super::{Distribution, Operation};

pub struct Group<'a, K> {
    pub keys: Vec<K>,
    pub map: HashMap<K, Box<dyn Operation + 'a>>,
}

impl<'a, K> Group<'a, K>
where
    K: Eq + Hash,
{
    #[must_use]
    pub fn new(keys: Vec<K>, map: HashMap<K, Box<dyn Operation + 'a>>) -> Self {
        Self { keys, map }
    }

    #[must_use]
    pub fn from_geometry<F>(
        geometry: &Geometry,
        key_of: F,
        map: HashMap<K, Box<dyn Operation + 'a>>,
    ) -> Self
    where
        F: Fn(usize, &Device) -> K,
    {
        let keys = (0..geometry.len())
            .map(|device| key_of(device, &geometry[device]))
            .collect();
        Self { keys, map }
    }

    fn route(&self, device: usize) -> Result<&dyn Operation, Error> {
        let key =
            self.keys
                .get(device)
                .ok_or(Error::InvalidPayload(PayloadError::GroupKeyMissing {
                    device,
                }))?;
        self.map
            .get(key)
            .map(AsRef::as_ref)
            .ok_or(Error::InvalidPayload(PayloadError::GroupKeyUnknown {
                device,
            }))
    }
}

impl<K> Operation for Group<'_, K>
where
    K: Eq + Hash,
{
    fn frames(&self) -> usize {
        self.map.values().map(|op| op.frames()).max().unwrap_or(1)
    }

    fn distribution(&self) -> Distribution {
        Distribution::PerDevice
    }

    fn encode(
        &self,
        device: usize,
        frame: usize,
        out: &mut [u8; PAYLOAD_BYTES],
    ) -> Result<Cmd, Error> {
        self.route(device)?.encode(device, frame, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::ConfigModulation;
    use crate::value::ModulationBank;

    #[test]
    fn group_routes_per_device() {
        let group = Group::new(
            vec![0usize, 1usize],
            HashMap::from([
                (
                    0usize,
                    Box::new(ConfigModulation {
                        bank: ModulationBank::B0,
                        divider: 1,
                        size: 1,
                    }) as Box<dyn Operation>,
                ),
                (
                    1usize,
                    Box::new(ConfigModulation {
                        bank: ModulationBank::B1,
                        divider: 1,
                        size: 1,
                    }) as Box<dyn Operation>,
                ),
            ]),
        );

        assert_eq!(group.distribution(), Distribution::PerDevice);
        assert_eq!(group.frames(), 1);

        let mut out0 = [0u8; PAYLOAD_BYTES];
        group.encode(0, 0, &mut out0).unwrap();
        assert_eq!(out0[0], 0, "device 0 -> bank B0");

        let mut out1 = [0u8; PAYLOAD_BYTES];
        group.encode(1, 0, &mut out1).unwrap();
        assert_eq!(out1[0], 1, "device 1 -> bank B1");
    }

    #[test]
    fn from_geometry_builds_keys() {
        use crate::geometry::{Autd3, Point3, UnitQuaternion};

        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let group = Group::from_geometry(
            &geo,
            |i, _dev| i,
            HashMap::from([
                (
                    0usize,
                    Box::new(ConfigModulation {
                        bank: ModulationBank::B0,
                        divider: 1,
                        size: 1,
                    }) as Box<dyn Operation>,
                ),
                (
                    1usize,
                    Box::new(ConfigModulation {
                        bank: ModulationBank::B1,
                        divider: 1,
                        size: 1,
                    }) as Box<dyn Operation>,
                ),
            ]),
        );
        assert_eq!(group.keys, vec![0, 1]);
    }

    #[test]
    fn group_rejects_uncovered_device() {
        let group: Group<usize> = Group::new(vec![7usize], HashMap::new());
        let mut out = [0u8; PAYLOAD_BYTES];
        assert!(matches!(
            group.encode(0, 0, &mut out),
            Err(Error::InvalidPayload(_))
        ));
    }
}
