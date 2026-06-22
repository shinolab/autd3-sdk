use core::f32::consts::PI;

use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};

#[must_use]
pub fn focus_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    wavelength: Length,
    intensity: Intensity,
) -> Emission {
    let dist = (target - position).norm();
    Emission {
        phase: Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad),
        intensity,
    }
}

pub fn focus_device(
    device: &Device,
    target: Point3<f32>,
    wavelength: Length,
    intensity: Intensity,
    out: &mut [Emission; NUM_TRANSDUCERS],
) {
    assert_eq!(device.len(), NUM_TRANSDUCERS, "not an AUTD3 device");
    for (e, &pos) in out.iter_mut().zip(device.positions()) {
        *e = focus_transducer(pos, target, wavelength, intensity);
    }
}

pub fn focus(
    geometry: &Geometry,
    target: Point3<f32>,
    wavelength: Length,
    intensity: Intensity,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) {
    assert_eq!(
        out.len(),
        geometry.len(),
        "out must have one slot per device"
    );
    for (slot, dev) in out.iter_mut().zip(geometry.iter()) {
        focus_device(dev, target, wavelength, intensity, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion};
    use autd3_rs_core::units::mm;

    use super::*;

    #[test]
    fn focus_transducer_phase_wraps_per_wavelength() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;

        let e = focus_transducer(
            dev.position(0),
            Point3::new(0.0, 0.0, 2.0 * lambda.mm()),
            lambda,
            Intensity::MAX,
        );
        assert_eq!(e.phase, Phase(0));
        assert_eq!(e.intensity, Intensity(0xFF));

        let e = focus_transducer(
            dev.position(0),
            Point3::new(0.0, 0.0, 2.25 * lambda.mm()),
            lambda,
            Intensity(0x80),
        );
        assert_eq!(e.phase, Phase(192));
        assert_eq!(e.intensity, Intensity(0x80));
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;

        let mut pattern = [Emission::default(); NUM_TRANSDUCERS];
        focus_device(&dev, target, lambda, Intensity::MAX, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i],
                focus_transducer(pos, target, lambda, Intensity::MAX)
            );
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

        let mut patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; geo.len()];
        focus(&geo, target, lambda, Intensity::MAX, &mut patterns);
        assert_eq!(patterns.len(), 2);
        for (pattern, dev) in patterns.iter().zip(&geo) {
            let mut expected = [Emission::default(); NUM_TRANSDUCERS];
            focus_device(dev, target, lambda, Intensity::MAX, &mut expected);
            assert_eq!(*pattern, expected);
        }
    }
}
