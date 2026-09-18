use core::f32::consts::PI;

use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitQuaternion, UnitVector3, Vector3};
use autd3_rs_core::value::{Emission, Phase};

fn rotation(dir: UnitVector3<f32>) -> UnitQuaternion<f32> {
    let v = Vector3::new(dir.y, -dir.x, 0.0);
    let theta_v = v.norm().asin();
    v.try_normalize(1.0e-6)
        .map_or_else(UnitQuaternion::identity, |v| {
            UnitQuaternion::new(v * -theta_v)
        })
}

fn bessel_phase(
    position: Point3<f32>,
    apex: Point3<f32>,
    rot: &UnitQuaternion<f32>,
    theta: Angle,
    wavelength: Length,
) -> Phase {
    let r = rot * (position - apex);
    let dist = theta.rad().cos() * r.xy().norm() - theta.rad().sin() * r.z;
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
    bessel_phase(position, apex, &rotation(direction), theta, wavelength)
}

pub fn bessel_device(
    device: &Device,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    dst: &mut [Emission],
) {
    let rot = rotation(direction);
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = bessel_phase(pos, apex, &rot, theta, wavelength);
    }
}

pub fn bessel(
    geometry: &Geometry,
    apex: Point3<f32>,
    direction: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
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
    use autd3_rs_core::value::Intensity;

    use super::*;

    #[test]
    fn bessel_phase_matches_formula() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(10.0, 20.0, 150.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.1, -0.2, 1.0));
        let theta = Angle::from_rad(0.3);

        let rot = {
            let v: Vector3<f32> = Vector3::new(dir.y, -dir.x, 0.0);
            let theta_v = v.norm().asin();
            v.try_normalize(1.0e-6)
                .map_or_else(UnitQuaternion::identity, |v| {
                    UnitQuaternion::new(v * -theta_v)
                })
        };

        for &pos in dev.positions() {
            let p = bessel_transducer(pos, apex, dir, theta, lambda);
            let r = rot * (pos - apex);
            let dist = theta.rad().cos() * (r.x * r.x + r.y * r.y).sqrt() - theta.rad().sin() * r.z;
            let expected = Phase::from(-dist / lambda.mm() * 2.0 * PI * rad);
            assert_eq!(p, expected);
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
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(30.0, 40.0, 120.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.2, 0.3, 1.0));
        let theta = Angle::from_rad(0.5);

        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        bessel_device(&dev, apex, dir, theta, lambda, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i].phase,
                bessel_transducer(pos, apex, dir, theta, lambda)
            );
            assert_eq!(pattern[i].intensity, Intensity(0x42));
        }
    }
}
