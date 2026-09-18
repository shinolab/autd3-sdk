use core::f32::consts::PI;

use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

#[must_use]
#[inline]
pub fn plane_transducer(
    position: Point3<f32>,
    direction: UnitVector3<f32>,
    wavelength: Length,
) -> Phase {
    Phase::from(-direction.dot(&position.coords) / wavelength.mm() * 2.0 * PI * rad)
}

pub fn plane_device(
    device: &Device,
    direction: UnitVector3<f32>,
    wavelength: Length,
    dst: &mut [Emission],
) {
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = plane_transducer(pos, direction, wavelength);
    }
}

pub fn plane(
    geometry: &Geometry,
    direction: UnitVector3<f32>,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        plane_device(dev, direction, wavelength, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;
    use autd3_rs_core::value::Intensity;

    use super::*;

    #[test]
    fn plane_phase_matches_dot_product() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 0.0, 1.0));

        for &pos in dev.positions() {
            let p = plane_transducer(pos, dir, lambda);
            let expected = Phase::from(-dir.dot(&pos.coords) / lambda.mm() * 2.0 * PI * rad);
            assert_eq!(p, expected);
        }
    }

    #[test]
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 1.0));

        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        plane_device(&dev, dir, lambda, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(pattern[i].phase, plane_transducer(pos, dir, lambda));
            assert_eq!(pattern[i].intensity, Intensity(0x42));
        }
    }

    #[test]
    fn geometry_level_matches_device_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(0.0, 1.0, 1.0));

        let mut emissions = geo.pattern_buffer();
        plane(&geo, dir, lambda, &mut emissions);
        let mut expected = geo.pattern_buffer();
        for (slot, dev) in expected.iter_mut().zip(&geo) {
            plane_device(dev, dir, lambda, slot);
        }
        assert_eq!(emissions, expected);
    }
}
