mod autd3;
mod autd3_unity;
mod device;

pub use autd3::Autd3;
pub use autd3_unity::Autd3Unity;
pub use device::Device;
pub use nalgebra::{Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3};

use crate::common::Length;
use crate::params::NUM_TRANSDUCERS;
use crate::value::Emission;

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
            devices: devices
                .into_iter()
                .enumerate()
                .map(|(i, d)| {
                    let mut device = d.into();
                    device.set_idx(i);
                    device
                })
                .collect(),
        }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.devices.len()
    }

    #[must_use]
    pub fn pattern_buffer(&self) -> Vec<[Emission; NUM_TRANSDUCERS]> {
        vec![[Emission::default(); NUM_TRANSDUCERS]; self.len()]
    }

    #[must_use]
    pub fn num_transducers(&self) -> usize {
        self.devices.iter().map(Device::len).sum()
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

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;
    use nalgebra::UnitQuaternion;

    use super::*;
    use crate::params::NUM_TRANSDUCERS;

    #[test]
    fn geometry_sets_device_idx_and_num_transducers() {
        let g = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        assert_eq!(g[0].idx(), 0);
        assert_eq!(g[1].idx(), 1);
        assert_eq!(g.num_transducers(), 2 * NUM_TRANSDUCERS);
    }

    #[test]
    fn device_basis_directions_for_identity() {
        let g = Geometry::new(vec![Autd3::default()]);
        let dev = &g[0];
        assert_abs_diff_eq!(dev.x_direction().into_inner(), Vector3::x(), epsilon = 1e-4);
        assert_abs_diff_eq!(dev.y_direction().into_inner(), Vector3::y(), epsilon = 1e-4);
        assert_abs_diff_eq!(
            dev.axial_direction().into_inner(),
            Vector3::z(),
            epsilon = 1e-4
        );
        assert_abs_diff_eq!(
            dev.rotation().angle_to(&UnitQuaternion::identity()),
            0.0,
            epsilon = 1e-4
        );
    }

    #[test]
    fn device_rotation_tracks_quarter_turn_about_x() {
        let rot = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), core::f32::consts::FRAC_PI_2);
        let g = Geometry::new(vec![Autd3::new(Point3::origin(), rot)]);
        let dev = &g[0];
        assert_abs_diff_eq!(
            dev.axial_direction().into_inner(),
            (rot * Vector3::z_axis()).into_inner(),
            epsilon = 1e-4
        );
        assert_abs_diff_eq!(dev.rotation().angle_to(&rot), 0.0, epsilon = 1e-4);
    }
}
