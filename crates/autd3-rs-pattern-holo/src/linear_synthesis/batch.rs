use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, TransducerMask};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::propagation::{
    batch_shape, enabled_transducers, quantize, target_amplitudes, validate_dst_len, wavenumber,
};

pub(crate) struct BatchSetup<'a> {
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_batched<B, P, I, F>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    setup: &BatchSetup<'_>,
    phases: &mut [P],
    intensities: &mut [I],
    solve: F,
) -> Result<(), HoloError>
where
    B: LinAlgBackend,
    P: AsMut<[Vec<Phase>]>,
    I: AsMut<[Vec<Intensity>]>,
    F: Fn(&B, &B::Matrix, &B::Vector, usize, usize) -> B::Vector,
{
    if phases.len() != intensities.len() {
        return Err(HoloError::DstProblemCountMismatch {
            phases: phases.len(),
            intensities: intensities.len(),
        });
    }
    let foci_per_problem = batch_shape(foci, phases.len())?;
    let mask = setup.mask;
    mask.validate(geometry)?;
    for (p, i) in phases.iter_mut().zip(intensities.iter_mut()) {
        validate_dst_len(p.as_mut().len(), geometry)?;
        validate_dst_len(i.as_mut().len(), geometry)?;
    }

    let k = wavenumber(wavelength);
    let (tr_pos, tr_dir) = enabled_transducers(geometry, mask);
    let enabled = tr_pos.len();
    let chunk = backend.max_batch(2 * foci_per_problem * enabled * 8).max(1);

    for (foci, (phases, intensities)) in foci
        .chunks(chunk.saturating_mul(foci_per_problem))
        .zip(phases.chunks_mut(chunk).zip(intensities.chunks_mut(chunk)))
    {
        let problems = phases.len();
        let g = backend.propagation_matrix(&tr_pos, &tr_dir, foci, problems, k, setup.directivity);
        let amps = target_amplitudes(backend, foci, problems);
        let q = solve(backend, &g, &amps, problems, enabled);
        quantize(
            backend,
            geometry,
            &q,
            setup.constraint,
            mask,
            setup.parallel,
            phases,
            intensities,
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry, TransducerMask, Vector3};
    use autd3_rs_core::value::{Intensity, Phase};

    use crate::amp::Pa;
    use crate::amplitude_target::AmplitudeTarget;
    use crate::backend::NalgebraBackend;
    use crate::error::HoloError;
    use crate::linear_synthesis::{GsOption, gs, gs_batch};
    use crate::test_utils::{ALGORITHMS, Batch, Slot, geometry, slot, wavelength};

    fn problem(g: &Geometry, seed: usize, nf: usize) -> Vec<AmplitudeTarget> {
        (0..nf)
            .map(|i| AmplitudeTarget {
                point: g.center()
                    + Vector3::new(
                        (seed + i) as f32 * 7.0,
                        seed as f32 * -3.0,
                        120.0 + i as f32 * 5.0,
                    ),
                amplitude: (3e3 + (seed * nf + i) as f32 * 100.0) * Pa,
            })
            .collect()
    }

    fn filled(g: &Geometry, phase: Phase, intensity: Intensity) -> Slot {
        (
            vec![vec![phase; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
            vec![vec![intensity; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
        )
    }

    fn batch(one: &Slot, n: usize) -> Batch {
        (vec![one.0.clone(); n], vec![one.1.clone(); n])
    }

    fn problems(b: &Batch) -> impl Iterator<Item = Slot> + '_ {
        b.0.iter().cloned().zip(b.1.iter().cloned())
    }

    #[test]
    fn batch_matches_sequential() {
        let g = geometry(2);
        for nf in [1usize, 4] {
            let owned: Vec<Vec<AmplitudeTarget>> = (0..5).map(|k| problem(&g, k, nf)).collect();
            let foci: Vec<AmplitudeTarget> = owned.concat();

            let mut batched = batch(&slot(&g), owned.len());
            let mut one = slot(&g);

            for (name, single, batched_fn) in ALGORITHMS {
                batched_fn(&g, &foci, TransducerMask::AllEnabled, &mut batched).unwrap();
                for (f, want) in owned.iter().zip(problems(&batched)) {
                    single(&g, f, TransducerMask::AllEnabled, &mut one).unwrap();
                    assert_eq!(one, want, "{name} {nf} foci");
                }
            }
        }
    }

    #[test]
    fn parallel_flag_does_not_change_the_result() {
        let g = geometry(2);
        let masked: Vec<Vec<bool>> = (0..g.num_devices())
            .map(|d| {
                (0..Autd3::NUM_TRANSDUCERS)
                    .map(|t| (d + t) % 3 != 0)
                    .collect()
            })
            .collect();
        let owned: Vec<Vec<AmplitudeTarget>> = (0..3).map(|k| problem(&g, k, 4)).collect();
        let foci: Vec<AmplitudeTarget> = owned.concat();

        for mask in [TransducerMask::AllEnabled, TransducerMask::Masked(&masked)] {
            let mut on = slot(&g);
            let mut off = slot(&g);
            gs(
                &NalgebraBackend,
                &g,
                &owned[0],
                wavelength(),
                &GsOption {
                    mask,
                    parallel: true,
                    ..Default::default()
                },
                &mut on.0,
                &mut on.1,
            )
            .unwrap();
            gs(
                &NalgebraBackend,
                &g,
                &owned[0],
                wavelength(),
                &GsOption {
                    mask,
                    parallel: false,
                    ..Default::default()
                },
                &mut off.0,
                &mut off.1,
            )
            .unwrap();
            assert_eq!(on, off, "single problem");

            let mut on = batch(&slot(&g), owned.len());
            let mut off = batch(&slot(&g), owned.len());
            gs_batch(
                &NalgebraBackend,
                &g,
                &foci,
                wavelength(),
                &GsOption {
                    mask,
                    parallel: true,
                    ..Default::default()
                },
                &mut on.0,
                &mut on.1,
            )
            .unwrap();
            gs_batch(
                &NalgebraBackend,
                &g,
                &foci,
                wavelength(),
                &GsOption {
                    mask,
                    parallel: false,
                    ..Default::default()
                },
                &mut off.0,
                &mut off.1,
            )
            .unwrap();
            assert_eq!(on, off, "batch");
        }
    }

    #[test]
    fn all_masked_batch_matches_sequential() {
        let g = geometry(2);
        let masked: Vec<Vec<bool>> = vec![vec![false; Autd3::NUM_TRANSDUCERS]; g.num_devices()];
        let mask = TransducerMask::Masked(&masked);
        let owned: Vec<Vec<AmplitudeTarget>> = (0..3).map(|k| problem(&g, k, 2)).collect();
        let foci: Vec<AmplitudeTarget> = owned.concat();

        let dirty = filled(&g, Phase(0x7F), Intensity(0xFF));
        let inactive = filled(&g, Phase::ZERO, Intensity::MIN);

        for (name, single, batched_fn) in ALGORITHMS {
            let mut one = dirty.clone();
            let mut batched = batch(&dirty, owned.len());
            single(&g, &owned[0], mask, &mut one).unwrap();
            batched_fn(&g, &foci, mask, &mut batched).unwrap();
            assert_eq!(one, inactive, "{name} single");
            assert!(problems(&batched).all(|b| b == one), "{name} batch");
        }
    }

    #[test]
    fn rejects_malformed_batches() {
        let g = geometry(1);
        let foci = problem(&g, 0, 5);
        let mut dst = batch(&slot(&g), 2);

        assert_eq!(
            gs_batch(
                &NalgebraBackend,
                &g,
                &foci,
                wavelength(),
                &GsOption::default(),
                &mut dst.0,
                &mut dst.1
            ),
            Err(HoloError::BatchSizeMismatch {
                foci: 5,
                problems: 2
            })
        );
        assert_eq!(
            gs_batch(
                &NalgebraBackend,
                &g,
                &foci,
                wavelength(),
                &GsOption::default(),
                &mut [],
                &mut []
            ),
            Err(HoloError::NoProblems)
        );
        assert_eq!(
            gs_batch(
                &NalgebraBackend,
                &g,
                &[],
                wavelength(),
                &GsOption::default(),
                &mut [slot(&g).0],
                &mut [slot(&g).1]
            ),
            Err(HoloError::NoFoci)
        );
        let mut two = batch(&slot(&g), 2);
        assert_eq!(
            gs_batch(
                &NalgebraBackend,
                &g,
                &problem(&g, 0, 2),
                wavelength(),
                &GsOption::default(),
                &mut two.0,
                &mut two.1[..1]
            ),
            Err(HoloError::DstProblemCountMismatch {
                phases: 2,
                intensities: 1
            })
        );
    }

    #[test]
    fn a_dst_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let g = geometry(2);
        let foci = problem(&g, 0, 2);
        let short = slot(&geometry(1));
        let want = Err(HoloError::DstDeviceCountMismatch {
            got: 1,
            expected: 2,
        });

        let mut one = short.clone();
        for (name, single, _) in ALGORITHMS {
            assert_eq!(
                single(&g, &foci, TransducerMask::AllEnabled, &mut one),
                want,
                "{name}"
            );
        }

        let mut phases = vec![slot(&g).0, short.0];
        let mut intensities = vec![slot(&g).1; 2];
        assert_eq!(
            gs_batch(
                &NalgebraBackend,
                &g,
                &problem(&g, 0, 4),
                wavelength(),
                &GsOption::default(),
                &mut phases,
                &mut intensities
            ),
            want
        );
    }
}
