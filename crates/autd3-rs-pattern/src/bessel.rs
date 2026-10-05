use core::f32::consts::PI;

use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::Phase;

fn bessel_phase(
    position: Point3<f32>,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    (sin, cos): (f32, f32),
    wavelength: Length,
) -> Phase {
    let r = position - apex;
    let z = direction.dot(&r);
    let rho = (r - direction.into_inner() * z).norm();
    let dist = cos * rho - sin * z;
    Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad)
}

#[must_use]
#[inline]
pub fn bessel_transducer(
    position: Point3<f32>,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
) -> Phase {
    bessel_phase(position, apex, direction, theta.rad().sin_cos(), wavelength)
}

pub fn bessel_device(
    device: &Device,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    dst: &mut [Phase],
) {
    let sin_cos = theta.rad().sin_cos();
    for (p, &pos) in dst.iter_mut().zip(device.positions()) {
        *p = bessel_phase(pos, apex, direction, sin_cos, wavelength);
    }
}

pub fn bessel(
    geometry: &Geometry,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    dst: &mut [Vec<Phase>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        bessel_device(dev, apex, direction, theta, wavelength, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Vector3};
    use autd3_rs_core::units::mm;

    use super::*;

    fn non_z_directions() -> [UnitVector3<f32>; 5] {
        [
            Vector3::new(0.3, 0.0, 1.0),
            Vector3::new(0.1, -0.2, 1.0),
            Vector3::new(1.0, 0.5, 0.0),
            Vector3::new(-0.4, 0.7, -1.0),
            Vector3::new(0.0, 0.0, -1.0),
        ]
        .map(UnitVector3::new_normalize)
    }

    fn perpendicular_basis(dir: UnitVector3<f32>) -> (Vector3<f32>, Vector3<f32>) {
        let seed = if dir.x.abs() < 0.9 {
            Vector3::x()
        } else {
            Vector3::y()
        };
        let u = dir.cross(&seed).normalize();
        let v = dir.cross(&u);
        (u, v)
    }

    fn assert_phase_close(actual: Phase, expected: Phase) {
        let diff = actual.0.wrapping_sub(expected.0);
        assert!(
            diff.min(diff.wrapping_neg()) <= 1,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn reversing_direction_negates_axial_phase() {
        let lambda = 8.5 * mm;
        let apex = Point3::origin();
        let pos = Point3::new(0.0, 0.0, 10.0);
        let theta = Angle::from_rad(0.3);

        let forward = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));
        let backward = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, -1.0));
        assert_eq!(
            bessel_transducer(pos, apex, forward, theta, lambda),
            Phase(0x59)
        );
        assert_eq!(
            bessel_transducer(pos, apex, backward, theta, lambda),
            Phase(0xA7)
        );
    }

    #[test]
    fn off_axis_point_matches_known_phase() {
        let lambda = 8.5 * mm;
        let apex = Point3::origin();
        let pos = Point3::new(30.0, 40.0, 10.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));
        let theta = Angle::from_rad(0.3);

        assert_eq!(
            bessel_transducer(pos, apex, dir, theta, lambda),
            Phase(0xBA)
        );
    }

    #[test]
    fn points_on_axis_have_zero_radius() {
        let lambda = 8.5 * mm;
        let apex = Point3::new(10.0, 20.0, 150.0);
        let theta = Angle::from_rad(0.3);

        for dir in non_z_directions() {
            for along in [-40.0f32, -5.0, 0.0, 12.5, 80.0] {
                let pos = apex + dir.into_inner() * along;
                let expected =
                    Phase::from(theta.rad().sin() * along / lambda.mm() * 2.0 * PI * rad);
                assert_phase_close(bessel_transducer(pos, apex, dir, theta, lambda), expected);
            }
        }
    }

    #[test]
    fn phase_is_symmetric_around_direction() {
        let lambda = 8.5 * mm;
        let apex = Point3::new(10.0, 20.0, 150.0);
        let theta = Angle::from_rad(0.3);

        for dir in non_z_directions() {
            let (u, v) = perpendicular_basis(dir);
            for (radius, along) in [(15.0f32, -60.0f32), (42.0, 30.0), (97.0, -140.0)] {
                let at = |azimuth: f32| {
                    let pos = apex
                        + u * (radius * azimuth.cos())
                        + v * (radius * azimuth.sin())
                        + dir.into_inner() * along;
                    bessel_transducer(pos, apex, dir, theta, lambda)
                };
                let reference = at(0.0);
                for step in 1..12u8 {
                    assert_phase_close(at(f32::from(step) * PI / 6.0), reference);
                }
            }
        }
    }

    #[test]
    fn rotating_whole_setup_keeps_phase() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let local_apex = Point3::new(10.0, 20.0, 150.0);
        let world_apex = Point3::new(-35.0, 60.0, 90.0);
        let theta = Angle::from_rad(0.3);
        let z_axis = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));

        for dir in non_z_directions() {
            let (u, v) = perpendicular_basis(dir);
            for &local in dev.positions() {
                let r = local - local_apex;
                let world = world_apex + u * r.x + v * r.y + dir.into_inner() * r.z;
                assert_phase_close(
                    bessel_transducer(world, world_apex, dir, theta, lambda),
                    bessel_transducer(local, local_apex, z_axis, theta, lambda),
                );
            }
        }
    }

    #[test]
    fn bessel_zero_half_cone_angle_is_radial() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(0.0, 0.0, 200.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));
        let theta = Angle::ZERO;

        let pos = dev.position(1);
        let p = bessel_transducer(pos, apex, dir, theta, lambda);
        let r = pos - apex;
        let rho = (r.x * r.x + r.y * r.y).sqrt();
        assert!(rho > 0.0);
        let expected = Phase::from(-rho / lambda.mm() * 2.0 * PI * rad);
        assert_eq!(p, expected);
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(30.0, 40.0, 120.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.2, 0.3, 1.0));
        let theta = Angle::from_rad(0.5);

        let mut pattern = vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS];
        bessel_device(&dev, apex, dir, theta, lambda, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(pattern[i], bessel_transducer(pos, apex, dir, theta, lambda));
        }
    }
}
