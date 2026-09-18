use core::f32::consts::PI;

use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{Device, Geometry, Point3, Vector3};
use autd3_rs_core::value::{Emission, Phase};

#[inline]
pub(crate) fn focus_phase(offset: Vector3<f32>, wavelength: Length) -> Angle {
    -offset.norm() / wavelength.mm() * 2.0 * PI * rad
}

#[must_use]
#[inline]
pub fn focus_transducer(position: Point3<f32>, target: Point3<f32>, wavelength: Length) -> Phase {
    Phase::from(focus_phase(position - target, wavelength))
}

pub fn focus_device(
    device: &Device,
    target: Point3<f32>,
    wavelength: Length,
    dst: &mut [Emission],
) {
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = focus_transducer(pos, target, wavelength);
    }
}

pub fn focus(
    geometry: &Geometry,
    target: Point3<f32>,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        focus_device(dev, target, wavelength, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion};
    use autd3_rs_core::units::mm;
    use autd3_rs_core::value::Intensity;

    use super::*;

    #[test]
    fn focus_transducer_phase_wraps_per_wavelength() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;

        let p = focus_transducer(
            dev.position(0),
            Point3::new(0.0, 0.0, 2.0 * lambda.mm()),
            lambda,
        );
        assert_eq!(p, Phase(0));

        let p = focus_transducer(
            dev.position(0),
            Point3::new(0.0, 0.0, 2.25 * lambda.mm()),
            lambda,
        );
        assert_eq!(p, Phase(192));
    }

    #[test]
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;

        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        focus_device(&dev, target, lambda, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(pattern[i].phase, focus_transducer(pos, target, lambda));
            assert_eq!(pattern[i].intensity, Intensity(0x42));
        }
    }

    #[test]
    fn geometry_level_matches_device_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let target = Point3::new(100.0, 66.0, 150.0);
        let lambda = 8.5 * mm;

        let mut emissions = geo.pattern_buffer();
        focus(&geo, target, lambda, &mut emissions);
        assert_eq!(emissions.len(), 2);
        for (actual, dev) in emissions.iter().zip(&geo) {
            let mut expected = vec![
                Emission {
                    phase: Phase::ZERO,
                    intensity: Intensity::MAX,
                };
                Autd3::NUM_TRANSDUCERS
            ];
            focus_device(dev, target, lambda, &mut expected);
            assert_eq!(*actual, expected);
        }
    }
}
