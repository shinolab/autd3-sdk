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
    batch_shape, enabled_transducers, quantize, target_amplitudes, wavenumber,
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
        crate::mask::validate_dst_len(p.as_mut().len(), geometry)?;
        crate::mask::validate_dst_len(i.as_mut().len(), geometry)?;
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
