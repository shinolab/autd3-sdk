use core::f32::consts::PI;

use autd3_rs_core::common::units::rad;
use autd3_rs_core::common::{Angle, Length};
use autd3_rs_core::geometry::{Geometry, Point3, Vector3};
use autd3_rs_core::value::Phase;

use crate::each::fill_each_transducer;

#[inline]
pub(crate) fn propagation_phase(distance_mm: f32, wavelength: Length) -> Angle {
    -distance_mm / wavelength.mm() * 2.0 * PI * rad
}

#[inline]
pub(crate) fn focus_phase(offset: Vector3<f32>, wavelength: Length) -> Angle {
    propagation_phase(offset.norm(), wavelength)
}

#[must_use]
#[inline]
pub fn focus_transducer(position: Point3<f32>, target: Point3<f32>, wavelength: Length) -> Phase {
    Phase::from(focus_phase(position - target, wavelength))
}

pub fn focus(geometry: &Geometry, target: Point3<f32>, wavelength: Length, dst: &mut [Vec<Phase>]) {
    fill_each_transducer(geometry, dst, |pos| {
        focus_transducer(pos, target, wavelength)
    });
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Device, UnitQuaternion};
    use autd3_rs_core::units::mm;

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
    fn geometry_level_matches_transducer_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let target = Point3::new(100.0, 66.0, 150.0);
        let lambda = 8.5 * mm;

        let mut phases = geo.phase_buffer();
        focus(&geo, target, lambda, &mut phases);
        assert_eq!(phases.len(), geo.num_devices());
        for (slot, dev) in phases.iter().zip(&geo) {
            assert_eq!(slot.len(), dev.num_transducers());
            for (&phase, &pos) in slot.iter().zip(dev.positions()) {
                assert_eq!(phase, focus_transducer(pos, target, lambda));
            }
        }
    }
}
