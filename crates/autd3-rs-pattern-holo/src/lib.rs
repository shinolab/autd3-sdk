mod amp;
mod backend;
mod combinatorial;
mod constraint;
mod control_point;
mod directivity;
mod error;
mod linear_synthesis;
mod mask;
mod propagation;

pub use amp::{Amplitude, Pa, dB, kPa};
pub use backend::{LinAlgBackend, NalgebraBackend};
pub use combinatorial::{GreedyOption, abs_objective_func, greedy};
pub use constraint::EmissionConstraint;
pub use control_point::ControlPoint;
pub use directivity::Directivity;
pub use error::HoloError;
pub use linear_synthesis::{GsOption, GspatOption, NaiveOption, gs, gspat, naive};
pub use mask::TransducerMask;

#[cfg(test)]
mod tests {
    use autd3_rs_core::common::units::{m, s};
    use autd3_rs_core::geometry::{Autd3, Geometry, Point3, UnitQuaternion};
    use autd3_rs_core::params::NUM_TRANSDUCERS;
    use autd3_rs_core::value::{Emission, Intensity};

    use super::*;

    fn wavelength() -> autd3_rs_core::common::Length {
        autd3_rs_pattern::wavelength(340.0 * m / s)
    }

    fn single_device() -> Geometry {
        Geometry::new(vec![Autd3::default()])
    }

    fn focus_target(geometry: &Geometry) -> Point3<f32> {
        geometry.center() + autd3_rs_core::geometry::Vector3::new(0.0, 0.0, 150.0)
    }

    fn buffer(geometry: &Geometry) -> Vec<[Emission; NUM_TRANSDUCERS]> {
        vec![[Emission::default(); NUM_TRANSDUCERS]; geometry.len()]
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let mut out = buffer(&geometry);
        assert_eq!(
            naive(
                &geometry,
                &[],
                wavelength(),
                &NaiveOption::default(),
                &backend,
                TransducerMask::AllEnabled,
                &mut out,
            ),
            Err(HoloError::NoFoci)
        );
    }

    #[test]
    fn out_must_match_geometry() {
        let geometry = Geometry::new(vec![
            Autd3::default(),
            Autd3::new(Point3::new(200.0, 0.0, 0.0), UnitQuaternion::identity()),
        ]);
        let backend = NalgebraBackend;
        let foci = [ControlPoint {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let mut out = buffer(&geometry);
        naive(
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption::default(),
            &backend,
            TransducerMask::AllEnabled,
            &mut out,
        )
        .unwrap();
        assert_eq!(out.len(), geometry.len());
        assert!(
            out.iter()
                .all(|slot| slot.iter().any(|e| *e != Emission::default()))
        );
    }

    #[test]
    fn uniform_constraint_sets_all_intensities() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let foci = [ControlPoint {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let mut out = buffer(&geometry);
        gspat(
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: EmissionConstraint::Uniform(Intensity(0x80)),
                ..Default::default()
            },
            &backend,
            TransducerMask::AllEnabled,
            &mut out,
        )
        .unwrap();
        assert!(out[0].iter().all(|e| e.intensity == Intensity(0x80)));
    }

    #[test]
    fn naive_single_focus_phases_match_focus_pattern() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let target = focus_target(&geometry);
        let foci = [ControlPoint {
            point: target,
            amplitude: 5e3 * Pa,
        }];

        let mut out = buffer(&geometry);
        naive(
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
            },
            &backend,
            TransducerMask::AllEnabled,
            &mut out,
        )
        .unwrap();

        let mut expected = buffer(&geometry);
        autd3_rs_pattern::focus(
            &geometry,
            target,
            wavelength(),
            &autd3_rs_pattern::FocusOption::default(),
            &mut expected,
        );

        for (a, b) in out[0].iter().zip(expected[0].iter()) {
            let diff = a.phase.0.wrapping_sub(b.phase.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {:?} vs {:?}", a.phase, b.phase);
        }
    }

    #[test]
    fn gspat_single_focus_phases_match_focus_pattern() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let target = focus_target(&geometry);
        let foci = [ControlPoint {
            point: target,
            amplitude: 5e3 * Pa,
        }];

        let mut out = buffer(&geometry);
        gspat(
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                ..Default::default()
            },
            &backend,
            TransducerMask::AllEnabled,
            &mut out,
        )
        .unwrap();

        let mut expected = buffer(&geometry);
        autd3_rs_pattern::focus(
            &geometry,
            target,
            wavelength(),
            &autd3_rs_pattern::FocusOption::default(),
            &mut expected,
        );

        for (a, b) in out[0].iter().zip(expected[0].iter()) {
            let diff = a.phase.0.wrapping_sub(b.phase.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {:?} vs {:?}", a.phase, b.phase);
        }
    }

    #[test]
    fn all_algorithms_focus_on_target() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let target = focus_target(&geometry);
        let foci = [ControlPoint {
            point: target,
            amplitude: 5e3 * Pa,
        }];
        let lambda = wavelength();

        let mut n = buffer(&geometry);
        let mut g = buffer(&geometry);
        let mut gp = buffer(&geometry);
        naive(
            &geometry,
            &foci,
            lambda,
            &NaiveOption::default(),
            &backend,
            TransducerMask::AllEnabled,
            &mut n,
        )
        .unwrap();
        gs(
            &geometry,
            &foci,
            lambda,
            &GsOption::default(),
            &backend,
            TransducerMask::AllEnabled,
            &mut g,
        )
        .unwrap();
        gspat(
            &geometry,
            &foci,
            lambda,
            &GspatOption::default(),
            &backend,
            TransducerMask::AllEnabled,
            &mut gp,
        )
        .unwrap();

        for out in [&n, &g, &gp] {
            assert!(out[0].iter().any(|e| e.intensity != Intensity::MIN));
            assert!(out[0].iter().any(|e| e.phase != out[0][0].phase));
        }
    }

    #[test]
    fn masked_transducers_are_null() {
        let geometry = single_device();
        let backend = NalgebraBackend;
        let foci = [ControlPoint {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];

        let mut enabled = [[true; NUM_TRANSDUCERS]; 1];
        for (t, slot) in enabled[0].iter_mut().enumerate() {
            *slot = t % 2 == 0;
        }
        let mask = TransducerMask::Masked(&enabled);

        let mut out = buffer(&geometry);
        naive(
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
            },
            &backend,
            mask,
            &mut out,
        )
        .unwrap();

        for (t, e) in out[0].iter().enumerate() {
            if t % 2 == 0 {
                assert_eq!(e.intensity, Intensity::MAX, "enabled transducer {t}");
            } else {
                assert_eq!(
                    *e,
                    Emission::default(),
                    "disabled transducer {t} must be NULL"
                );
            }
        }
    }
}
