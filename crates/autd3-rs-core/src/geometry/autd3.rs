use nalgebra::{Point3, UnitQuaternion, Vector3};

use super::Device;

#[derive(Clone, Debug, PartialEq)]
pub struct Autd3 {
    pub origin: Point3<f32>,
    pub rotation: UnitQuaternion<f32>,
}

impl Autd3 {
    pub const NUM_TRANSDUCERS: usize = autd3_cpu_wire::fpga_params::NUM_TRANSDUCERS;
    pub const GRID_X: u32 = 18;
    pub const GRID_Y: u32 = 14;
    pub const PITCH_MM: f32 = 10.16;
    pub const DEVICE_WIDTH: f32 = 192.0;
    pub const DEVICE_HEIGHT: f32 = 151.4;
    pub const ROTATION_NORM_TOLERANCE: f32 = 1e-3;

    #[must_use]
    pub fn new(origin: Point3<f32>, rotation: UnitQuaternion<f32>) -> Self {
        Self { origin, rotation }
    }
}

impl Default for Autd3 {
    fn default() -> Self {
        Self::new(Point3::origin(), UnitQuaternion::identity())
    }
}

impl From<Autd3> for Device {
    fn from(a: Autd3) -> Device {
        let positions: Vec<_> = (0..Autd3::GRID_Y)
            .flat_map(|y| (0..Autd3::GRID_X).map(move |x| (x, y)))
            .filter(|&(x, y)| !(y == 1 && (x == 1 || x == 2 || x == 16)))
            .map(|(x, y)| {
                a.origin
                    + a.rotation
                        * Vector3::new(x as f32 * Autd3::PITCH_MM, y as f32 * Autd3::PITCH_MM, 0.0)
            })
            .collect();
        debug_assert_eq!(positions.len(), Autd3::NUM_TRANSDUCERS);
        Device::new(a.rotation, positions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autd3_has_249_transducers_in_fpga_order() {
        let dev: Device = Autd3::new(Point3::origin(), UnitQuaternion::identity()).into();
        assert_eq!(dev.num_transducers(), Autd3::NUM_TRANSDUCERS);

        assert_eq!(dev.position(0), Point3::origin());
        assert_eq!(
            dev.position(17),
            Point3::new(17.0 * Autd3::PITCH_MM, 0.0, 0.0)
        );
        assert_eq!(dev.position(18), Point3::new(0.0, Autd3::PITCH_MM, 0.0));
        assert_eq!(
            dev.position(19),
            Point3::new(3.0 * Autd3::PITCH_MM, Autd3::PITCH_MM, 0.0)
        );
    }

    #[test]
    fn autd3_translates_by_origin() {
        let origin = Point3::new(100.0, 200.0, 300.0);
        let dev: Device = Autd3::new(origin, UnitQuaternion::identity()).into();
        assert_eq!(dev.position(0), origin);
    }

    #[test]
    fn autd3_default_direction_is_z() {
        let dev: Device = Autd3::new(Point3::origin(), UnitQuaternion::identity()).into();
        assert_eq!(dev.axial_direction(), Vector3::z_axis());
    }

    #[test]
    fn autd3_rotates_positions_and_direction() {
        let rot = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f32::consts::FRAC_PI_2);
        let dev: Device = Autd3 {
            origin: Point3::origin(),
            rotation: rot,
        }
        .into();

        approx::assert_abs_diff_eq!(
            dev.position(1),
            Point3::new(Autd3::PITCH_MM, 0.0, 0.0),
            epsilon = 1e-4
        );
        approx::assert_abs_diff_eq!(
            *dev.axial_direction(),
            Vector3::new(0.0, -1.0, 0.0),
            epsilon = 1e-4
        );
    }

    #[test]
    fn device_center_is_array_center() {
        let dev: Device = Autd3::new(Point3::origin(), UnitQuaternion::identity()).into();
        let c = dev.center();
        approx::assert_abs_diff_eq!(c.x, 86.36, epsilon = 1.0);
        approx::assert_abs_diff_eq!(c.y, 66.04, epsilon = 1.0);
        approx::assert_abs_diff_eq!(c.z, 0.0);
    }

    #[test]
    fn autd3_default_is_origin_no_rotation() {
        let dev: Device = Autd3::default().into();
        assert_eq!(dev.position(0), Point3::origin());
        assert_eq!(dev.axial_direction(), Vector3::z_axis());
    }
}
