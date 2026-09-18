use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Phase};

use crate::focus::focus_phase;

fn twin_trap_phase(
    position: Point3<f32>,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
) -> Phase {
    let r = position - target;
    let phase = Phase::from(focus_phase(r, wavelength));
    if normal.dot(&r) >= 0.0 {
        phase + Phase::PI
    } else {
        phase
    }
}

#[must_use]
#[inline]
pub fn twin_trap_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
) -> Phase {
    twin_trap_phase(position, target, normal, wavelength)
}

pub fn twin_trap_device(
    device: &Device,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
    dst: &mut [Emission],
) {
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        e.phase = twin_trap_transducer(pos, target, normal, wavelength);
    }
}

pub fn twin_trap(
    geometry: &Geometry,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        twin_trap_device(dev, target, normal, wavelength, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;
    use autd3_rs_core::value::Intensity;

    use super::*;
    use crate::focus_transducer;

    #[test]
    fn split_plane_sides_differ_by_pi() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let normal = Vector3::x_axis();

        let mut positive = None;
        let mut negative = None;
        for &pos in dev.positions() {
            let p = twin_trap_transducer(pos, target, normal, lambda);
            let base = focus_transducer(pos, target, lambda);
            if normal.dot(&(pos - target)) >= 0.0 {
                assert_eq!(p, base + Phase::PI);
                positive = Some(());
            } else {
                assert_eq!(p, base);
                negative = Some(());
            }
        }
        assert!(positive.is_some() && negative.is_some());
    }

    #[test]
    fn mirrored_transducers_are_pi_apart() {
        let lambda = 8.5 * mm;
        let target = Point3::new(0.0, 0.0, 150.0);
        let normal = Vector3::x_axis();

        for d in [1.0_f32, 8.0, 32.0, 64.0] {
            let left = twin_trap_transducer(Point3::new(-d, 20.0, 0.0), target, normal, lambda);
            let right = twin_trap_transducer(Point3::new(d, 20.0, 0.0), target, normal, lambda);
            assert_eq!(right, left + Phase::PI);
        }
    }

    #[test]
    fn normal_flip_swaps_the_lobes() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let normal = Vector3::x_axis();
        let flipped = UnitVector3::new_normalize(Vector3::new(-1.0, 0.0, 0.0));

        for &pos in dev.positions() {
            if (pos.x - target.x).abs() < 1.0e-3 {
                continue;
            }
            let a = twin_trap_transducer(pos, target, normal, lambda);
            let b = twin_trap_transducer(pos, target, flipped, lambda);
            assert_eq!(a, b + Phase::PI);
        }
    }

    #[test]
    fn device_level_matches_transducer_level_and_keeps_intensity() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;
        let normal = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 0.0));

        let mut pattern = vec![
            Emission {
                phase: Phase::ZERO,
                intensity: Intensity(0x42),
            };
            Autd3::NUM_TRANSDUCERS
        ];
        twin_trap_device(&dev, target, normal, lambda, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i].phase,
                twin_trap_transducer(pos, target, normal, lambda)
            );
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
        let normal = Vector3::x_axis();

        let mut emissions = geo.pattern_buffer();
        twin_trap(&geo, target, normal, lambda, &mut emissions);
        let mut expected = geo.pattern_buffer();
        for (slot, dev) in expected.iter_mut().zip(&geo) {
            twin_trap_device(dev, target, normal, lambda, slot);
        }
        assert_eq!(emissions, expected);
    }
}
