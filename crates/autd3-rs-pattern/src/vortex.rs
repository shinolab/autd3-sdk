use autd3_rs_core::common::Length;
use autd3_rs_core::common::units::rad;
use autd3_rs_core::geometry::{Device, Geometry, Point3, UnitVector3, Vector3};
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::focus::focus_phase;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VortexOption {
    pub intensity: Intensity,
    pub phase_offset: Phase,
}

impl Default for VortexOption {
    fn default() -> Self {
        Self {
            intensity: Intensity::MAX,
            phase_offset: Phase::ZERO,
        }
    }
}

struct AzimuthBasis {
    u: Vector3<f32>,
    v: Vector3<f32>,
}

fn azimuth_basis(axis: UnitVector3<f32>) -> AzimuthBasis {
    let a = axis.into_inner();
    let seed = if a.x.abs() <= a.y.abs() && a.x.abs() <= a.z.abs() {
        Vector3::x()
    } else if a.y.abs() <= a.z.abs() {
        Vector3::y()
    } else {
        Vector3::z()
    };
    let u = (seed - a * a.dot(&seed)).normalize();
    let v = a.cross(&u);
    AzimuthBasis { u, v }
}

fn vortex_phase(
    position: Point3<f32>,
    target: Point3<f32>,
    basis: &AzimuthBasis,
    order: i32,
    wavelength: Length,
) -> Phase {
    let r = position - target;
    let theta = basis.v.dot(&r).atan2(basis.u.dot(&r));
    Phase::from(focus_phase(r, wavelength) + order as f32 * theta * rad)
}

#[must_use]
pub fn vortex_transducer(
    position: Point3<f32>,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    order: i32,
    wavelength: Length,
    option: &VortexOption,
) -> Emission {
    Emission {
        phase: vortex_phase(position, target, &azimuth_basis(axis), order, wavelength)
            + option.phase_offset,
        intensity: option.intensity,
    }
}

pub fn vortex_device(
    device: &Device,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    order: i32,
    wavelength: Length,
    option: &VortexOption,
    dst: &mut [Emission],
) {
    let basis = azimuth_basis(axis);
    for (e, &pos) in dst.iter_mut().zip(device.positions()) {
        *e = Emission {
            phase: vortex_phase(pos, target, &basis, order, wavelength) + option.phase_offset,
            intensity: option.intensity,
        };
    }
}

pub fn vortex(
    geometry: &Geometry,
    target: Point3<f32>,
    axis: UnitVector3<f32>,
    order: i32,
    wavelength: Length,
    option: &VortexOption,
    dst: &mut [Vec<Emission>],
) {
    assert_eq!(
        dst.len(),
        geometry.num_devices(),
        "dst must have one slot per device"
    );
    for (slot, dev) in dst.iter_mut().zip(geometry.iter()) {
        vortex_device(dev, target, axis, order, wavelength, option, slot);
    }
}

#[cfg(test)]
mod tests {
    use core::f32::consts::FRAC_PI_2;

    use autd3_rs_core::geometry::{Autd3, UnitQuaternion};
    use autd3_rs_core::units::mm;

    use super::*;
    use crate::{FocusOption, focus_transducer};

    #[test]
    fn order_zero_matches_focus() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = dev.center() + Vector3::new(0.0, 0.0, 150.0);
        let option = VortexOption::default();
        let focus_option = FocusOption {
            intensity: option.intensity,
            phase_offset: option.phase_offset,
        };

        for &pos in dev.positions() {
            assert_eq!(
                vortex_transducer(pos, target, Vector3::z_axis(), 0, lambda, &option),
                focus_transducer(pos, target, lambda, &focus_option)
            );
        }
    }

    #[test]
    fn azimuth_advances_by_order_times_theta() {
        let lambda = 8.5 * mm;
        let target = Point3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let radius = 40.0_f32;
        let option = VortexOption::default();

        for order in [1_i32, 2, -1] {
            let base = vortex_transducer(
                Point3::new(radius, 0.0, 0.0),
                target,
                axis,
                order,
                lambda,
                &option,
            );
            let quarter = vortex_transducer(
                Point3::new(0.0, radius, 0.0),
                target,
                axis,
                order,
                lambda,
                &option,
            );
            let expected = base.phase + Phase::from(order as f32 * FRAC_PI_2 * rad);
            assert_eq!(quarter.phase, expected);
        }
    }

    #[test]
    fn opposite_azimuth_differs_by_pi() {
        let lambda = 8.5 * mm;
        let target = Point3::new(0.0, 0.0, 150.0);
        let axis = Vector3::z_axis();
        let radius = 40.0_f32;
        let option = VortexOption::default();

        let a = vortex_transducer(
            Point3::new(radius, 0.0, 0.0),
            target,
            axis,
            1,
            lambda,
            &option,
        );
        let b = vortex_transducer(
            Point3::new(-radius, 0.0, 0.0),
            target,
            axis,
            1,
            lambda,
            &option,
        );
        assert_eq!(b.phase, a.phase + Phase::PI);
    }

    #[test]
    fn azimuth_basis_is_orthonormal_and_deterministic() {
        for axis in [
            Vector3::x_axis(),
            Vector3::y_axis(),
            Vector3::z_axis(),
            UnitVector3::new_normalize(Vector3::new(1.0, 2.0, 3.0)),
            UnitVector3::new_normalize(Vector3::new(-0.3, 0.9, -0.1)),
        ] {
            let basis = azimuth_basis(axis);
            let again = azimuth_basis(axis);
            assert_eq!(basis.u, again.u);
            assert_eq!(basis.v, again.v);
            approx::assert_abs_diff_eq!(basis.u.norm(), 1.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(basis.v.norm(), 1.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(basis.u.dot(&basis.v), 0.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(axis.dot(&basis.u), 0.0, epsilon = 1.0e-5);
            approx::assert_abs_diff_eq!(axis.dot(&basis.v), 0.0, epsilon = 1.0e-5);
        }
    }

    #[test]
    fn default_axis_uses_the_conventional_azimuth() {
        let basis = azimuth_basis(Vector3::z_axis());
        assert_eq!(basis.u, Vector3::x());
        assert_eq!(basis.v, Vector3::y());
    }

    #[test]
    fn phase_offset_is_applied() {
        let dev: Device = Autd3::default().into();
        let lambda = 8.5 * mm;
        let target = Point3::new(10.0, 20.0, 150.0);
        let axis = Vector3::z_axis();
        let offset = Phase(0x25);

        let base = vortex_transducer(
            dev.position(0),
            target,
            axis,
            1,
            lambda,
            &VortexOption::default(),
        );
        let shifted = vortex_transducer(
            dev.position(0),
            target,
            axis,
            1,
            lambda,
            &VortexOption {
                phase_offset: offset,
                ..Default::default()
            },
        );
        assert_eq!(shifted.phase, base.phase + offset);
    }

    #[test]
    fn device_level_matches_transducer_level() {
        let dev: Device = Autd3::default().into();
        let target = Point3::new(86.36, 66.04, 150.0);
        let lambda = 8.5 * mm;
        let axis = UnitVector3::new_normalize(Vector3::new(0.1, -0.2, 1.0));
        let option = VortexOption {
            intensity: Intensity(0x80),
            phase_offset: Phase(0x10),
        };

        let mut pattern = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
        vortex_device(&dev, target, axis, 2, lambda, &option, &mut pattern);
        for (i, &pos) in dev.positions().iter().enumerate() {
            assert_eq!(
                pattern[i],
                vortex_transducer(pos, target, axis, 2, lambda, &option)
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
        let axis = Vector3::z_axis();
        let option = VortexOption::default();

        let mut emissions =
            vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geo.num_devices()];
        vortex(&geo, target, axis, 1, lambda, &option, &mut emissions);
        assert_eq!(emissions.len(), 2);
        for (actual, dev) in emissions.iter().zip(&geo) {
            let mut expected = vec![Emission::default(); Autd3::NUM_TRANSDUCERS];
            vortex_device(dev, target, axis, 1, lambda, &option, &mut expected);
            assert_eq!(*actual, expected);
        }
    }
}
