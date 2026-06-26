use nalgebra::{Point3, UnitQuaternion, Vector3};

use crate::params::NUM_TRANSDUCERS;

use super::Device;

pub struct Autd3 {
    pub origin: Point3<f32>,
    pub rotation: UnitQuaternion<f32>,
}

impl Autd3 {
    pub const GRID_X: u32 = 18;
    pub const GRID_Y: u32 = 14;
    pub const PITCH_MM: f32 = 10.16;
    pub const DEVICE_WIDTH: f32 = 192.0;
    pub const DEVICE_HEIGHT: f32 = 151.4;

    #[must_use]
    pub fn new(origin: Point3<f32>, rotation: UnitQuaternion<f32>) -> Self {
        Self { origin, rotation }
    }
}

pub(crate) const fn is_missing_transducer(x: u32, y: u32) -> bool {
    y == 1 && (x == 1 || x == 2 || x == 16)
}

impl Default for Autd3 {
    fn default() -> Self {
        Self::new(Point3::origin(), UnitQuaternion::identity())
    }
}

impl From<Autd3> for Device {
    fn from(a: Autd3) -> Device {
        let direction = a.rotation * Vector3::z_axis();
        let mut positions = Vec::with_capacity(NUM_TRANSDUCERS);
        let mut directions = Vec::with_capacity(NUM_TRANSDUCERS);
        for y in 0..Autd3::GRID_Y {
            for x in 0..Autd3::GRID_X {
                if !is_missing_transducer(x, y) {
                    positions.push(
                        a.origin
                            + a.rotation
                                * Vector3::new(
                                    x as f32 * Autd3::PITCH_MM,
                                    y as f32 * Autd3::PITCH_MM,
                                    0.0,
                                ),
                    );
                    directions.push(direction);
                }
            }
        }
        debug_assert_eq!(positions.len(), NUM_TRANSDUCERS);
        Device::new(a.rotation, positions, directions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autd3_has_249_transducers_in_fpga_order() {
        let dev: Device = Autd3::new(Point3::origin(), UnitQuaternion::identity()).into();
        assert_eq!(dev.len(), NUM_TRANSDUCERS);

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
        let z = Vector3::z_axis();
        for &dir in dev.directions() {
            assert_eq!(dir, z);
        }
    }

    #[test]
    fn autd3_rotates_positions_and_direction() {
        let rot = UnitQuaternion::from_axis_angle(&Vector3::x_axis(), std::f32::consts::FRAC_PI_2);
        let dev: Device = Autd3 {
            origin: Point3::origin(),
            rotation: rot,
        }
        .into();

        let p1 = dev.position(1);
        assert!((p1.x - Autd3::PITCH_MM).abs() < 1e-4);
        assert!(p1.y.abs() < 1e-4);
        assert!(p1.z.abs() < 1e-4);

        let dir = dev.direction(0);
        assert!(dir.x.abs() < 1e-4);
        assert!((dir.y + 1.0).abs() < 1e-4, "y={}", dir.y);
        assert!(dir.z.abs() < 1e-4);
    }

    #[test]
    fn device_center_is_array_center() {
        let dev: Device = Autd3::new(Point3::origin(), UnitQuaternion::identity()).into();
        let c = dev.center();
        assert!((c.x - 86.36).abs() < 1.0, "x center ≈ 86.36, got {}", c.x);
        assert!((c.y - 66.04).abs() < 1.0, "y center ≈ 66.04, got {}", c.y);
        assert!(c.z.abs() < f32::EPSILON);
    }

    #[test]
    fn autd3_default_is_origin_no_rotation() {
        let dev: Device = Autd3::default().into();
        assert_eq!(dev.position(0), Point3::origin());
        assert_eq!(dev.direction(0), Vector3::z_axis());
    }
}
