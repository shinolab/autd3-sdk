mod autd3;
mod autd3_unity;
mod device;

pub use autd3::Autd3;
pub use autd3_unity::Autd3Unity;
pub use device::Device;
pub use nalgebra::{Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3};

use crate::common::Length;

#[must_use]
pub fn point(x: Length, y: Length, z: Length) -> Point3<f32> {
    Point3::new(x.mm(), y.mm(), z.mm())
}

#[must_use]
pub fn offset(x: Length, y: Length, z: Length) -> Vector3<f32> {
    Vector3::new(x.mm(), y.mm(), z.mm())
}

#[derive(Clone, Debug)]
pub struct Geometry {
    devices: Vec<Device>,
}

impl Geometry {
    #[must_use]
    pub fn new<D: Into<Device>>(devices: Vec<D>) -> Self {
        Self {
            devices: devices.into_iter().map(Into::into).collect(),
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.devices.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    pub fn iter(&self) -> core::slice::Iter<'_, Device> {
        self.devices.iter()
    }

    #[must_use]
    pub fn center(&self) -> Point3<f32> {
        let n = self.devices.len() as f32;
        let sum = self
            .devices
            .iter()
            .fold(Vector3::zeros(), |acc, d| acc + d.center().coords);
        Point3::from(sum / n)
    }
}

impl core::ops::Index<usize> for Geometry {
    type Output = Device;

    fn index(&self, index: usize) -> &Device {
        &self.devices[index]
    }
}

impl<'a> IntoIterator for &'a Geometry {
    type Item = &'a Device;
    type IntoIter = core::slice::Iter<'a, Device>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
