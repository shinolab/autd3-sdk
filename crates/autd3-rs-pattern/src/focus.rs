use core::f32::consts::PI;

use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusOption {
    pub intensity: Intensity,
    pub phase_offset: Phase,
}

impl Default for FocusOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
    }
}

#[must_use]
pub fn focus_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    wavelength: Length,
    option: &FocusOption,
) -> Emission {
    let dist = (target - position).norm();
    Emission {
        phase: Phase::from(-dist / wavelength.mm() * 2.0 * PI * rad) + option.phase_offset,
        intensity: option.intensity,
    }
}

pub fn focus_device(
    device: &Device,
    target: Point3<f32>,
    wavelength: Length,
    option: &FocusOption,
    out: &mut [Emission; NUM_TRANSDUCERS],
) {
    assert_eq!(device.len(), NUM_TRANSDUCERS, "not an AUTD3 device");
    for (e, &pos) in out.iter_mut().zip(device.positions()) {
        *e = focus_transducer(pos, target, wavelength, option);
    }
}

pub fn focus(
    geometry: &Geometry,
    target: Point3<f32>,
    wavelength: Length,
    option: &FocusOption,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) {
    assert_eq!(
        out.len(),
        geometry.len(),
        "out must have one slot per device"
    );
    for (slot, dev) in out.iter_mut().zip(geometry.iter()) {
        focus_device(dev, target, wavelength, option, slot);
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
            &FocusOption::default(),
        );
        assert_eq!(e.phase, Phase(0));
        assert_eq!(e.intensity, Intensity(0xFF));

        let e = focus_transducer(
            dev.position(0),
            Point3::new(0.0, 0.0, 2.25 * lambda.mm()),
            lambda,
            &FocusOption {
                intensity: Intensity(0x80),
                phase_offset: Phase::ZERO,
            },
        );
        assert_eq!(e.phase, Phase(192));
        assert_eq!(e.intensity, Intensity(0x80));
    }

    #[test]
    fn focus_phase_offset_is_applied() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = Point3::new(10.0, 20.0, 150.0);
        let offset = Phase(0x25);

        let base = focus_transducer(dev.position(0), target, lambda, &FocusOption::default());
        let shifted = focus_transducer(
            dev.position(0),
            target,
            lambda,
            &FocusOption {
                intensity: Intensity::MAX,
                phase_offset: offset,
            },
        );
        assert_eq!(shifted.phase, base.phase + offset);
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;
        let option = FocusOption::default();

        let mut pattern = [Emission::default(); NUM_TRANSDUCERS];
        focus_device(&dev, target, lambda, &option, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(pattern[i], focus_transducer(pos, target, lambda, &option));
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
        let option = FocusOption::default();

        let mut patterns = vec![[Emission::default(); NUM_TRANSDUCERS]; geo.len()];
        focus(&geo, target, lambda, &option, &mut patterns);
        assert_eq!(patterns.len(), 2);
        for (pattern, dev) in patterns.iter().zip(&geo) {
            let mut expected = [Emission::default(); NUM_TRANSDUCERS];
            focus_device(dev, target, lambda, &option, &mut expected);
            assert_eq!(*pattern, expected);
        }
    }
}
