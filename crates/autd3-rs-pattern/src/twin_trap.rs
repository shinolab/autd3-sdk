use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::focus::focus_phase;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwinTrapOption {
    pub intensity: Intensity,
    pub phase_offset: Phase,
}

impl Default for TwinTrapOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
    }
}

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
    option: &TwinTrapOption,
) -> Emission {
    Emission {
        phase: twin_trap_phase(position, target, normal, wavelength) + option.phase_offset,
        intensity: option.intensity,
    }
}

pub fn twin_trap_device(
    device: &Device,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
    option: &TwinTrapOption,
    dst: &mut [Emission],
) {
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        *e = twin_trap_transducer(pos, target, normal, wavelength, option);
    }
}

pub fn twin_trap(
    geometry: &Geometry,
    target: Point3<f32>,
    normal: UnitVector3<f32>,
    wavelength: Length,
    option: &TwinTrapOption,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        twin_trap_device(dev, target, normal, wavelength, option, slot);
    }
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;

    use super::*;
    use crate::{FocusOption, focus_transducer};

    #[test]
    fn split_plane_sides_differ_by_pi() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let normal = Vector3::x_axis();
        let option = TwinTrapOption::default();

        let mut positive = None;
        let mut negative = None;
        for &pos in dev.positions() {
            let e = twin_trap_transducer(pos, target, normal, lambda, &option);
            let base = focus_transducer(pos, target, lambda, &FocusOption::default());
            if normal.dot(&(pos - target)) >= 0.0 {
                assert_eq!(e.phase, base.phase + Phase::PI);
                positive = Some(());
            } else {
                assert_eq!(e.phase, base.phase);
                negative = Some(());
            }
            assert_eq!(e.intensity, Intensity::MAX);
        }
        assert!(positive.is_some() && negative.is_some());
    }

    #[test]
    fn mirrored_transducers_are_pi_apart() {
        let lambda = 8.5 * mm;
        let target = Point3::new(0.0, 0.0, 150.0);
        let normal = Vector3::x_axis();
        let option = TwinTrapOption::default();

        for d in [1.0_f32, 8.0, 32.0, 64.0] {
            let left =
                twin_trap_transducer(Point3::new(-d, 20.0, 0.0), target, normal, lambda, &option);
            let right =
                twin_trap_transducer(Point3::new(d, 20.0, 0.0), target, normal, lambda, &option);
            assert_eq!(right.phase, left.phase + Phase::PI);
        }
    }

    #[test]
    fn phase_offset_is_applied() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = Point3::new(10.0, 20.0, 150.0);
        let normal = Vector3::x_axis();
        let offset = Phase(0x25);

        let base = twin_trap_transducer(
            dev.position(0),
            target,
            normal,
            lambda,
            &TwinTrapOption::default(),
        );
        let shifted = twin_trap_transducer(
            dev.position(0),
            target,
            normal,
            lambda,
            &TwinTrapOption {
                phase_offset: offset,
                ..Default::default()
            },
        );
        assert_eq!(shifted.phase, base.phase + offset);
    }

    #[test]
    fn normal_flip_swaps_the_lobes() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let option = TwinTrapOption::default();

        let normal = Vector3::x_axis();
        let flipped = UnitVector3::new_normalize(Vector3::new(-1.0, 0.0, 0.0));
        for &pos in dev.positions() {
            if (pos.x - target.x).abs() < 1.0e-3 {
                continue;
            }
            let a = twin_trap_transducer(pos, target, normal, lambda, &option);
            let b = twin_trap_transducer(pos, target, flipped, lambda, &option);
            assert_eq!(a.phase, b.phase + Phase::PI);
        }
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;
        let normal = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 0.0));
        let option = TwinTrapOption {
            intensity: Intensity(0x80),
            phase_offset: Phase(0x10),
        };

        let mut pattern = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
        twin_trap_device(&dev, target, normal, lambda, &option, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i],
                twin_trap_transducer(pos, target, normal, lambda, &option)
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
        let normal = Vector3::x_axis();
        let option = TwinTrapOption::default();

        let mut emissions =
            vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geo.num_devices()];
        twin_trap(&geo, target, normal, lambda, &option, &mut emissions);
        assert_eq!(emissions.len(), 2);
        for (actual, dev) in emissions.iter().zip(&geo) {
            let mut expected = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
            twin_trap_device(dev, target, normal, lambda, &option, &mut expected);
            assert_eq!(*actual, expected);
        }
    }
}
