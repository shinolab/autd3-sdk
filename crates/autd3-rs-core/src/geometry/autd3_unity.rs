use nalgebra::{Point3, Quaternion, UnitQuaternion, UnitVector3, Vector3};

use crate::params::NUM_TRANSDUCERS;

use super::autd3::is_missing_transducer;
use super::{Autd3, Device};

pub struct Autd3Unity {
    pub origin: Point3<f32>,
    pub rotation: Quaternion<f32>,
}

impl From<Autd3Unity> for Device {
    fn from(u: Autd3Unity) -> Device {
        let rot = UnitQuaternion::new_normalize(u.rotation);
        let origin_rh = Point3::new(
            u.origin.x * 1000.0,
            u.origin.y * 1000.0,
            -u.origin.z * 1000.0,
        );

        let unity_z = (rot * Vector3::z_axis()).into_inner();
        let dir_rh = UnitVector3::new_normalize(Vector3::new(unity_z.x, unity_z.y, -unity_z.z));

        let mut positions = Vec::with_capacity(NUM_TRANSDUCERS);
        let mut directions = Vec::with_capacity(NUM_TRANSDUCERS);
        for y in 0..Autd3::GRID_Y {
            for x in 0..Autd3::GRID_X {
                if !is_missing_transducer(x, y) {
                    let local =
                        Vector3::new(x as f32 * Autd3::PITCH_MM, y as f32 * Autd3::PITCH_MM, 0.0);
                    let unity_offset = rot * local;
                    positions.push(
                        origin_rh + Vector3::new(unity_offset.x, unity_offset.y, -unity_offset.z),
                    );
                    directions.push(dir_rh);
                }
            }
        }
        debug_assert_eq!(positions.len(), NUM_TRANSDUCERS);
        Device::new(positions, directions)
    }
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use super::*;

    #[test]
    fn unity_origin_converts_to_mm_and_flips_z() {
        let dev: Device = Autd3Unity {
            origin: Point3::new(0.1, 0.2, 0.3),
            rotation: Quaternion::identity(),
        }
        .into();
        let p = dev.position(0);
        assert_abs_diff_eq!(p.x, 100.0, epsilon = 1e-3);
        assert_abs_diff_eq!(p.y, 200.0, epsilon = 1e-3);
        assert_abs_diff_eq!(p.z, -300.0, epsilon = 1e-3);
    }

    #[test]
    fn unity_transducer_pitch_is_in_mm() {
        let dev: Device = Autd3Unity {
            origin: Point3::origin(),
            rotation: Quaternion::identity(),
        }
        .into();
        let p1 = dev.position(1);
        assert_abs_diff_eq!(p1.x, Autd3::PITCH_MM, epsilon = 1e-3);
        assert_abs_diff_eq!(p1.y, 0.0, epsilon = 1e-3);
        assert_abs_diff_eq!(p1.z, 0.0, epsilon = 1e-3);
    }

    #[test]
    fn unity_identity_rotation_direction_is_neg_z() {
        let dev: Device = Autd3Unity {
            origin: Point3::origin(),
            rotation: Quaternion::identity(),
        }
        .into();
        let dir = dev.direction(0);
        assert_abs_diff_eq!(dir.x, 0.0, epsilon = 1e-4);
        assert_abs_diff_eq!(dir.y, 0.0, epsilon = 1e-4);
        assert_abs_diff_eq!(dir.z, -1.0, epsilon = 1e-4);
    }

    #[test]
    fn unity_90deg_around_y_rotates_positions_and_direction() {
        let q = UnitQuaternion::from_axis_angle(&Vector3::y_axis(), std::f32::consts::FRAC_PI_2);
        let dev: Device = Autd3Unity {
            origin: Point3::origin(),
            rotation: *q.quaternion(),
        }
        .into();

        let p1 = dev.position(1);
        assert_abs_diff_eq!(p1.x, 0.0, epsilon = 1e-3);
        assert_abs_diff_eq!(p1.y, 0.0, epsilon = 1e-3);
        assert_abs_diff_eq!(p1.z, Autd3::PITCH_MM, epsilon = 1e-3);

        let dir = dev.direction(0);
        assert_abs_diff_eq!(dir.x, 1.0, epsilon = 1e-4);
        assert_abs_diff_eq!(dir.y, 0.0, epsilon = 1e-4);
        assert_abs_diff_eq!(dir.z, 0.0, epsilon = 1e-4);
    }
}
