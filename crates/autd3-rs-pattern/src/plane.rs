use core::f32::consts::PI;

use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaneOption {
    pub intensity: Intensity,
    pub phase_offset: Phase,
}

impl Default for PlaneOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
    }
}

#[must_use]
pub fn plane_transducer(
    position: Point3<f32>,
    dir: UnitVector3<f32>,
    wavelength: Length,
    option: &PlaneOption,
) -> Emission {
    Emission {
        phase: Phase::from(-dir.dot(&position.coords) / wavelength.mm() * 2.0 * PI * rad)
            + option.phase_offset,
        intensity: option.intensity,
    }
}

pub fn plane_device(
    device: &Device,
    dir: UnitVector3<f32>,
    wavelength: Length,
    option: &PlaneOption,
    out: &mut [Emission; NUM_TRANSDUCERS],
) {
    assert_eq!(device.len(), NUM_TRANSDUCERS, "not an AUTD3 device");
    for (e, &pos) in out.iter_mut().zip(device.positions()) {
        *e = plane_transducer(pos, dir, wavelength, option);
    }
}

pub fn plane(
    geometry: &Geometry,
    dir: UnitVector3<f32>,
    wavelength: Length,
    option: &PlaneOption,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) {
    assert_eq!(
        out.len(),
        geometry.len(),
        "out must have one slot per device"
    );
    for (slot, dev) in out.iter_mut().zip(geometry.iter()) {
        plane_device(dev, dir, wavelength, option, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;

    use super::*;

    #[test]
    fn plane_phase_matches_dot_product() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));

        for &pos in dev.positions() {
            let e = plane_transducer(pos, dir, lambda, &PlaneOption::default());
            let expected =
                Phase::from(-dir.dot(&pos.coords) / lambda.mm() * 2.0 * PI * rad) + Phase::ZERO;
            assert_eq!(e.phase, expected);
            assert_eq!(e.intensity, Intensity::MAX);
        }
    }

    #[test]
    fn plane_phase_offset_is_applied() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 1.0));
        let offset = Phase(0x30);

        let pos = dev.position(0);
        let base = plane_transducer(pos, dir, lambda, &PlaneOption::default());
        let shifted = plane_transducer(
            pos,
            dir,
            lambda,
            &PlaneOption {
                intensity: Intensity::MAX,
                phase_offset: offset,
            },
        );
        assert_eq!(shifted.phase, base.phase + offset);
    }

    #[test]
    fn geometry_level_matches_device_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 1.0, 1.0));
        let option = PlaneOption::default();

        let mut emissions = vec![[Emission::default(); NUM_TRANSDUCERS]; geo.len()];
        plane(&geo, dir, lambda, &option, &mut emissions);
        for (actual, dev) in emissions.iter().zip(&geo) {
            let mut expected = [Emission::default(); NUM_TRANSDUCERS];
            plane_device(dev, dir, lambda, &option, &mut expected);
            assert_eq!(*actual, expected);
        }
    }
}
