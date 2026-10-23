use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, Point3, UnitVector3};
use autd3_rs_core::value::Phase;

use crate::each::fill_each_transducer;
use crate::focus::propagation_phase;

#[must_use]
#[inline]
pub fn plane_transducer(
    position: Point3<f32>,
    direction: UnitVector3<f32>,
    wavelength: Length,
) -> Phase {
    Phase::from(propagation_phase(
        direction.dot(&position.coords),
        wavelength,
    ))
}

pub fn plane(
    geometry: &Geometry,
    direction: UnitVector3<f32>,
    wavelength: Length,
    dst: &mut [Vec<Phase>],
) {
    fill_each_transducer(geometry, dst, |pos| {
        plane_transducer(pos, direction, wavelength)
    });
}

#[cfg(test)]
mod tests {
    use core::f32::consts::PI;

    use autd3_rs_core::common::units::rad;
    use autd3_rs_core::geometry::{Autd3, Device, UnitQuaternion, Vector3};
    use autd3_rs_core::units::mm;

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
    fn geometry_level_matches_transducer_level() {
        let geo = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let lambda = 8.5 * mm;
        let dir = UnitVector3::new_normalize(Vector3::new(1.0, 1.0, 1.0));

        let mut phases = geo.phase_buffer();
        plane(&geo, dir, lambda, &mut phases);
        assert_eq!(phases.len(), geo.num_devices());
        for (slot, dev) in phases.iter().zip(&geo) {
            assert_eq!(slot.len(), dev.num_transducers());
            for (&phase, &pos) in slot.iter().zip(dev.positions()) {
                assert_eq!(phase, plane_transducer(pos, dir, lambda));
            }
        }
    }
}
