use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, TransducerMask};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::propagation::{
    enabled_transducers, quantize, reject_focus_on_enabled_transducer, target_amplitudes,
    validate_dst_len, wavenumber,
};

pub(crate) struct SynthesisSetup<'a> {
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn synthesize<B, F>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    setup: &SynthesisSetup<'_>,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
    solve: F,
) -> Result<(), HoloError>
where
    B: LinAlgBackend,
    F: FnOnce(&B, &B::Matrix, &B::Vector, usize) -> B::Vector,
{
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    let mask = setup.mask;
    mask.validate(geometry)?;
    validate_dst_len(phases, geometry)?;
    validate_dst_len(intensities, geometry)?;
    reject_focus_on_enabled_transducer(geometry, mask, foci)?;

    let k = wavenumber(wavelength);
    let (tr_pos, tr_dir) = enabled_transducers(geometry, mask);
    let enabled = tr_pos.len();
    let g = backend.propagation_matrix(&tr_pos, &tr_dir, foci, k, setup.directivity);
    let amps = target_amplitudes(backend, foci);
    let q = solve(backend, &g, &amps, enabled);
    quantize(
        backend,
        &q,
        setup.constraint,
        mask,
        enabled,
        phases,
        intensities,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Geometry, TransducerMask, Vector3};
    use autd3_rs_core::value::{Intensity, Phase};

    use crate::amp::Pa;
    use crate::amplitude_target::AmplitudeTarget;
    use crate::error::HoloError;
    use crate::test_utils::{ALGORITHMS, Slot, geometry, slot};

    fn problem(g: &Geometry, nf: usize) -> Vec<AmplitudeTarget> {
        (0..nf)
            .map(|i| AmplitudeTarget {
                point: g.center() + Vector3::new(i as f32 * 7.0, 0.0, 120.0 + i as f32 * 5.0),
                amplitude: (3e3 + i as f32 * 100.0) * Pa,
            })
            .collect()
    }

    fn filled(g: &Geometry, phase: Phase, intensity: Intensity) -> Slot {
        (
            vec![vec![phase; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
            vec![vec![intensity; Autd3::NUM_TRANSDUCERS]; g.num_devices()],
        )
    }

    #[test]
    fn all_masked_writes_the_inactive_value_everywhere() {
        let g = geometry(2);
        let masked: Vec<Vec<bool>> = vec![vec![false; Autd3::NUM_TRANSDUCERS]; g.num_devices()];
        let mask = TransducerMask::Masked(&masked);
        let foci = problem(&g, 2);

        let inactive = filled(&g, Phase::ZERO, Intensity::MIN);

        for (name, single) in ALGORITHMS {
            let mut dst = filled(&g, Phase(0x7F), Intensity(0xFF));
            single(&g, &foci, mask, &mut dst).unwrap();
            assert_eq!(dst, inactive, "{name}");
        }
    }

    #[test]
    fn a_focus_on_an_enabled_transducer_is_an_error_and_on_a_masked_one_is_not() {
        let g = geometry(2);
        let mut foci = problem(&g, 3);
        foci[2].point = g[1].positions()[5];

        let mut enabled = vec![vec![true; Autd3::NUM_TRANSDUCERS]; g.num_devices()];
        enabled[1][5] = false;

        for (name, single) in ALGORITHMS {
            let mut dst = slot(&g);
            assert_eq!(
                single(&g, &foci, TransducerMask::AllEnabled, &mut dst),
                Err(HoloError::FocusOnTransducer {
                    focus: 2,
                    device: 1,
                    transducer: 5,
                }),
                "{name}"
            );

            single(&g, &foci, TransducerMask::Masked(&enabled), &mut dst).unwrap();
            assert!(
                dst.1.iter().flatten().any(|&i| i != Intensity::MIN),
                "{name}"
            );
        }
    }

    #[test]
    fn a_dst_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let g = geometry(2);
        let foci = problem(&g, 2);
        let want = Err(HoloError::DstDeviceCountMismatch {
            got: 1,
            expected: 2,
        });

        for (name, single) in ALGORITHMS {
            let mut short = slot(&geometry(1));
            assert_eq!(
                single(&g, &foci, TransducerMask::AllEnabled, &mut short),
                want,
                "{name} phases"
            );
            let mut short_intensities = (slot(&g).0, slot(&geometry(1)).1);
            assert_eq!(
                single(
                    &g,
                    &foci,
                    TransducerMask::AllEnabled,
                    &mut short_intensities
                ),
                want,
                "{name} intensities"
            );

            let mut long_row = slot(&g);
            long_row.1[1].push(Intensity::MIN);
            assert_eq!(
                single(&g, &foci, TransducerMask::AllEnabled, &mut long_row),
                Err(HoloError::DstTransducerCountMismatch {
                    device: 1,
                    got: Autd3::NUM_TRANSDUCERS + 1,
                    expected: Autd3::NUM_TRANSDUCERS,
                }),
                "{name} intensity row"
            );
        }
    }
}
