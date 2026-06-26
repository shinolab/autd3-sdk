use core::f32::consts::PI;

use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitQuaternion, UnitVector3, Vector3};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BesselOption {
    pub intensity: Intensity,
    pub phase_offset: Phase,
}

impl Default for BesselOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
    }
}

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
    let dist = theta.radian().cos() * r.xy().norm() - theta.radian().sin() * r.z;
    Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad)
}

#[must_use]
pub fn bessel_transducer(
    position: Point3<f32>,
    apex: Point3<f32>,
    dir: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    option: &BesselOption,
) -> Emission {
    let rot = rotation(dir);
    Emission {
        phase: bessel_phase(position, apex, &rot, theta, wavelength) + option.phase_offset,
        intensity: option.intensity,
    }
}

pub fn bessel_device(
    device: &Device,
    apex: Point3<f32>,
    dir: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    option: &BesselOption,
    out: &mut [Emission; NUM_TRANSDUCERS],
) {
    assert_eq!(device.len(), NUM_TRANSDUCERS, "not an AUTD3 device");
    let rot = rotation(dir);
    for (e, &pos) in out.iter_mut().zip(device.positions()) {
        *e = Emission {
            phase: bessel_phase(pos, apex, &rot, theta, wavelength) + option.phase_offset,
            intensity: option.intensity,
        };
    }
}

pub fn bessel(
    geometry: &Geometry,
    apex: Point3<f32>,
    dir: UnitVector3<f32>,
    theta: Angle,
    wavelength: Length,
    option: &BesselOption,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) {
    assert_eq!(
        out.len(),
        geometry.len(),
        "out must have one slot per device"
    );
    for (slot, dev) in out.iter_mut().zip(geometry.iter()) {
        bessel_device(dev, apex, dir, theta, wavelength, option, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Vector3};
    use autd3_rs_core::units::mm;

    use super::*;

    #[test]
    fn bessel_phase_matches_formula() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(10.0, 20.0, 150.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.1, -0.2, 1.0));
        let theta = Angle::from_radian(0.3);

        let rot = {
            let v: Vector3<f32> = Vector3::new(dir.y, -dir.x, 0.0);
            let theta_v = v.norm().asin();
            v.try_normalize(1.0e-6)
                .map_or_else(UnitQuaternion::identity, |v| {
                    UnitQuaternion::new(v * -theta_v)
                })
        };

        for &pos in dev.positions() {
            let e = bessel_transducer(pos, apex, dir, theta, lambda, &BesselOption::default());
            let r = rot * (pos - apex);
            let dist =
                theta.radian().cos() * (r.x * r.x + r.y * r.y).sqrt() - theta.radian().sin() * r.z;
            let expected = Phase::from(-dist / lambda.mm() * 2.0 * PI * rad) + Phase::ZERO;
            assert_eq!(e.phase, expected);
            assert_eq!(e.intensity, Intensity::MAX);
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
        let e = bessel_transducer(pos, apex, dir, theta, lambda, &BesselOption::default());
        let r = pos - apex;
        let rho = (r.x * r.x + r.y * r.y).sqrt();
        assert!(rho > 0.0);
        let expected = Phase::from(-rho / lambda.mm() * 2.0 * PI * rad);
        assert_eq!(e.phase, expected);
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let apex = Point3::new(30.0, 40.0, 120.0);
        let dir = UnitVector3::new_normalize(Vector3::new(0.2, 0.3, 1.0));
        let theta = Angle::from_radian(0.5);
        let option = BesselOption {
            intensity: Intensity::MAX,
            phase_offset: Phase(0x20),
        };

        let mut pattern = [Emission::default(); NUM_TRANSDUCERS];
        bessel_device(&dev, apex, dir, theta, lambda, &option, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i],
                bessel_transducer(pos, apex, dir, theta, lambda, &option)
            );
        }
    }
}
