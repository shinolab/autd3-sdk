mod amp;
mod amplitude_target;
mod backend;
mod combinatorial;
mod constraint;
mod directivity;
mod error;
mod linear_synthesis;
mod mask;
mod propagation;

pub use nalgebra;

pub use amp::{Amplitude, Pa, dB, kPa};
pub use amplitude_target::AmplitudeTarget;
pub use backend::{LinAlgBackend, NalgebraBackend};
pub use combinatorial::{GreedyOption, abs_objective_func, greedy};
pub use constraint::EmissionConstraint;
pub use directivity::Directivity;
pub use error::HoloError;
pub use linear_synthesis::{
    GsOption, GspatOption, NaiveOption, gs, gs_batch, gspat, gspat_batch, naive, naive_batch,
};

#[cfg(test)]
mod tests {
    use autd3_rs_core::common::units::{m, s};
    use autd3_rs_core::geometry::{
        Autd3, Geometry, Point3, TransducerGroups, TransducerMask, UnitQuaternion,
    };
    use autd3_rs_core::value::{Emission, Intensity, Phase};

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

    fn buffer(geometry: &Geometry) -> Vec<Vec<Emission>> {
        vec![vec![Emission::default(); Autd3::NUM_TRANSDUCERS]; geometry.num_devices()]
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = single_device();
        let mut dst = buffer(&geometry);
        assert_eq!(
            naive(
                &NalgebraBackend,
                &geometry,
                &[],
                wavelength(),
                &NaiveOption::default(),
                &mut dst,
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
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let mut dst = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption::default(),
            &mut dst,
        )
        .unwrap();
        assert_eq!(dst.len(), geometry.num_devices());
        assert!(
            dst.iter()
                .all(|slot| slot.iter().any(|e| *e != Emission::default()))
        );
    }

    #[test]
    fn uniform_constraint_sets_all_intensities() {
        let geometry = single_device();
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let mut dst = buffer(&geometry);
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: EmissionConstraint::Uniform(Intensity(0x80)),
                ..Default::default()
            },
            &mut dst,
        )
        .unwrap();
        assert!(dst[0].iter().all(|e| e.intensity == Intensity(0x80)));
    }

    #[test]
    fn naive_single_focus_phases_match_focus_pattern() {
        let geometry = single_device();
        let target = focus_target(&geometry);
        let foci = [AmplitudeTarget {
            point: target,
            amplitude: 5e3 * Pa,
        }];

        let mut dst = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                ..Default::default()
            },
            &mut dst,
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

        for (a, b) in dst[0].iter().zip(expected[0].iter()) {
            let diff = a.phase.0.wrapping_sub(b.phase.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {:?} vs {:?}", a.phase, b.phase);
        }
    }

    #[test]
    fn gspat_single_focus_phases_match_focus_pattern() {
        let geometry = single_device();
        let target = focus_target(&geometry);
        let foci = [AmplitudeTarget {
            point: target,
            amplitude: 5e3 * Pa,
        }];

        let mut dst = buffer(&geometry);
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                ..Default::default()
            },
            &mut dst,
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

        for (a, b) in dst[0].iter().zip(expected[0].iter()) {
            let diff = a.phase.0.wrapping_sub(b.phase.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {:?} vs {:?}", a.phase, b.phase);
        }
    }

    #[test]
    fn all_algorithms_focus_on_target() {
        let geometry = single_device();
        let target = focus_target(&geometry);
        let foci = [AmplitudeTarget {
            point: target,
            amplitude: 5e3 * Pa,
        }];
        let lambda = wavelength();

        let mut n = buffer(&geometry);
        let mut g = buffer(&geometry);
        let mut gp = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            lambda,
            &NaiveOption::default(),
            &mut n,
        )
        .unwrap();
        gs(
            &NalgebraBackend,
            &geometry,
            &foci,
            lambda,
            &GsOption::default(),
            &mut g,
        )
        .unwrap();
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            lambda,
            &GspatOption::default(),
            &mut gp,
        )
        .unwrap();

        for dst in [&n, &g, &gp] {
            assert!(dst[0].iter().any(|e| e.intensity != Intensity::MIN));
            assert!(dst[0].iter().any(|e| e.phase != dst[0][0].phase));
        }
    }

    #[test]
    fn masked_transducers_are_null() {
        let geometry = single_device();
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];

        let mut enabled = vec![vec![true; Autd3::NUM_TRANSDUCERS]; 1];
        for (t, slot) in enabled[0].iter_mut().enumerate() {
            *slot = t % 2 == 0;
        }
        let mask = TransducerMask::Masked(&enabled);

        let mut dst = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                mask,
                ..Default::default()
            },
            &mut dst,
        )
        .unwrap();

        for (t, e) in dst[0].iter().enumerate() {
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

    #[test]
    fn group_mask_restricts_the_optimization_to_the_group() {
        let geometry = single_device();
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let groups = TransducerGroups::new(&geometry, |_, tr| Some(tr % 2));

        let mut dst = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: EmissionConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                mask: groups.mask(1).unwrap(),
                ..Default::default()
            },
            &mut dst,
        )
        .unwrap();

        for (t, e) in dst[0].iter().enumerate() {
            if t % 2 == 1 {
                assert_eq!(e.intensity, Intensity::MAX, "group transducer {t}");
            } else {
                assert_eq!(*e, Emission::default(), "transducer {t} outside the group");
            }
        }
    }

    #[test]
    fn group_compute_optimizes_each_group_over_its_own_transducers() {
        let geometry = single_device();
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let groups = TransducerGroups::new(&geometry, |_, tr| Some(tr % 2 == 0));
        let other = Emission {
            phase: Phase(0x40),
            intensity: Intensity(0x20),
        };

        let mut dst = buffer(&geometry);
        autd3_rs_pattern::group_compute(
            &geometry,
            &groups,
            |even, mask, buffer| {
                if even {
                    naive(
                        &NalgebraBackend,
                        &geometry,
                        &foci,
                        wavelength(),
                        &NaiveOption {
                            constraint: EmissionConstraint::Uniform(Intensity::MAX),
                            directivity: Directivity::Sphere,
                            mask,
                            ..Default::default()
                        },
                        buffer,
                    )
                } else {
                    autd3_rs_pattern::uniform(other, buffer);
                    Ok(())
                }
            },
            &mut dst,
        )
        .unwrap();

        for (t, e) in dst[0].iter().enumerate() {
            if t % 2 == 0 {
                assert_eq!(e.intensity, Intensity::MAX, "even transducer {t}");
            } else {
                assert_eq!(*e, other, "odd transducer {t}");
            }
        }
    }
}
