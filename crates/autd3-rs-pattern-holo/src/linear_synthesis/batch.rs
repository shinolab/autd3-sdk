use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::value::{Intensity, Phase};

use crate::amplitude_target::AmplitudeTarget;
use crate::backend::LinAlgBackend;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::{
    batch_shape, batch_target_amplitudes, enabled_transducers, quantize_batch, wavenumber,
};

pub(crate) struct BatchSetup<'a> {
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn solve_batched<B, F>(
    backend: &B,
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    setup: &BatchSetup<'_>,
    phases: &mut [Vec<Vec<Phase>>],
    intensities: &mut [Vec<Vec<Intensity>>],
    solve: F,
) -> Result<(), HoloError>
where
    B: LinAlgBackend,
    F: Fn(&B, &B::BatchMatrix, &B::BatchVector, usize, usize) -> B::BatchVector,
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
    for (p, i) in phases.iter().zip(intensities.iter()) {
        crate::mask::validate_dst_len(p.len(), geometry)?;
        crate::mask::validate_dst_len(i.len(), geometry)?;
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
        let g = backend.batch_propagation_matrix(
            &tr_pos,
            &tr_dir,
            foci,
            problems,
            k,
            setup.directivity,
        );
        let amps = batch_target_amplitudes(backend, foci, problems);
        let q = solve(backend, &g, &amps, problems, enabled);
        quantize_batch(
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
