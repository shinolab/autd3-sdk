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
pub use constraint::IntensityConstraint;
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
    use autd3_rs_core::value::{Intensity, Phase};

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

    type Buffers = (Vec<Vec<Phase>>, Vec<Vec<Intensity>>);

    fn buffer(geometry: &Geometry) -> Buffers {
        (geometry.phase_buffer(), geometry.intensity_buffer())
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = single_device();
        let (mut phases, mut intensities) = buffer(&geometry);
        assert_eq!(
            naive(
                &NalgebraBackend,
                &geometry,
                &[],
                wavelength(),
                &NaiveOption::default(),
                &mut phases,
                &mut intensities,
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
        let (mut phases, mut intensities) = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption::default(),
            &mut phases,
            &mut intensities,
        )
        .unwrap();
        assert_eq!(phases.len(), geometry.num_devices());
        assert_eq!(intensities.len(), geometry.num_devices());
        assert!(
            intensities
                .iter()
                .all(|slot| slot.iter().any(|&i| i != Intensity::MIN))
        );
    }

    #[test]
    fn uniform_constraint_sets_all_intensities() {
        let geometry = single_device();
        let foci = [AmplitudeTarget {
            point: focus_target(&geometry),
            amplitude: 5e3 * Pa,
        }];
        let (mut phases, mut intensities) = buffer(&geometry);
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: IntensityConstraint::Uniform(Intensity(0x80)),
                ..Default::default()
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();
        assert!(intensities[0].iter().all(|&i| i == Intensity(0x80)));
    }

    #[test]
    fn naive_single_focus_phases_match_focus_pattern() {
        let geometry = single_device();
        let target = focus_target(&geometry);
        let foci = [AmplitudeTarget {
            point: target,
            amplitude: 5e3 * Pa,
        }];

        let (mut phases, mut intensities) = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: IntensityConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                ..Default::default()
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();

        let mut expected = geometry.phase_buffer();
        autd3_rs_pattern::focus(&geometry, target, wavelength(), &mut expected);

        for (a, b) in phases[0].iter().zip(expected[0].iter()) {
            let diff = a.0.wrapping_sub(b.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {a:?} vs {b:?}");
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

        let (mut phases, mut intensities) = buffer(&geometry);
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &GspatOption {
                constraint: IntensityConstraint::Uniform(Intensity::MAX),
                ..Default::default()
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();

        let mut expected = geometry.phase_buffer();
        autd3_rs_pattern::focus(&geometry, target, wavelength(), &mut expected);

        for (a, b) in phases[0].iter().zip(expected[0].iter()) {
            let diff = a.0.wrapping_sub(b.0);
            let diff = diff.min(0u8.wrapping_sub(diff));
            assert!(diff <= 1, "phase mismatch: {a:?} vs {b:?}");
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
            &mut n.0,
            &mut n.1,
        )
        .unwrap();
        gs(
            &NalgebraBackend,
            &geometry,
            &foci,
            lambda,
            &GsOption::default(),
            &mut g.0,
            &mut g.1,
        )
        .unwrap();
        gspat(
            &NalgebraBackend,
            &geometry,
            &foci,
            lambda,
            &GspatOption::default(),
            &mut gp.0,
            &mut gp.1,
        )
        .unwrap();

        for (phases, intensities) in [&n, &g, &gp] {
            assert!(intensities[0].iter().any(|&i| i != Intensity::MIN));
            assert!(phases[0].iter().any(|&p| p != phases[0][0]));
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

        let (mut phases, mut intensities) = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: IntensityConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                mask,
                ..Default::default()
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();

        for (t, (&p, &i)) in phases[0].iter().zip(&intensities[0]).enumerate() {
            if t % 2 == 0 {
                assert_eq!(i, Intensity::MAX, "enabled transducer {t}");
            } else {
                assert_eq!(p, Phase::ZERO, "disabled transducer {t} must be silent");
                assert_eq!(i, Intensity::MIN, "disabled transducer {t} must be silent");
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

        let (mut phases, mut intensities) = buffer(&geometry);
        naive(
            &NalgebraBackend,
            &geometry,
            &foci,
            wavelength(),
            &NaiveOption {
                constraint: IntensityConstraint::Uniform(Intensity::MAX),
                directivity: Directivity::Sphere,
                mask: groups.mask(1).unwrap(),
                ..Default::default()
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();

        for (t, (&p, &i)) in phases[0].iter().zip(&intensities[0]).enumerate() {
            if t % 2 == 1 {
                assert_eq!(i, Intensity::MAX, "group transducer {t}");
            } else {
                assert_eq!(p, Phase::ZERO, "transducer {t} outside the group");
                assert_eq!(i, Intensity::MIN, "transducer {t} outside the group");
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
        let other = (Phase(0x40), Intensity(0x20));

        let (mut phases, mut intensities) = buffer(&geometry);
        autd3_rs_pattern::group_compute(
            &geometry,
            &groups,
            |even, mask, phases, intensities| {
                if even {
                    naive(
                        &NalgebraBackend,
                        &geometry,
                        &foci,
                        wavelength(),
                        &NaiveOption {
                            constraint: IntensityConstraint::Uniform(Intensity::MAX),
                            directivity: Directivity::Sphere,
                            mask,
                            ..Default::default()
                        },
                        phases,
                        intensities,
                    )
                } else {
                    autd3_rs_pattern::set_phase(other.0, phases);
                    autd3_rs_pattern::set_intensity(other.1, intensities);
                    Ok(())
                }
            },
            &mut phases,
            &mut intensities,
        )
        .unwrap();

        for (t, (&p, &i)) in phases[0].iter().zip(&intensities[0]).enumerate() {
            if t % 2 == 0 {
                assert_eq!(i, Intensity::MAX, "even transducer {t}");
            } else {
                assert_eq!((p, i), other, "odd transducer {t}");
            }
        }
    }
}
