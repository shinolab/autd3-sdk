use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
use autd3_rs_core::value::Emission;

use crate::backend::LinAlgBackend;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::{
    batch_shape, batch_target_amplitudes, enabled_transducers, quantize_batch, wavenumber,
};

pub(crate) struct BatchSetup<'a> {
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub mask: TransducerMask<'a>,
    pub parallel: bool,
}

pub(crate) fn solve_batched<B, F>(
    backend: &B,
    geometry: &Geometry,
    foci: &[&[ControlPoint]],
    wavelength: Length,
    setup: &BatchSetup<'_>,
    dst: &mut [Vec<Vec<Emission>>],
    solve: F,
) -> Result<(), HoloError>
where
    B: LinAlgBackend,
    F: Fn(&B, &B::BatchMatrix, &B::BatchVector, usize, usize) -> B::BatchVector,
{
    let foci_per_problem = batch_shape(foci, dst.len())?;
    let mask = setup.mask;
    mask.validate(geometry);

    let k = wavenumber(wavelength);
    let (tr_pos, tr_dir) = enabled_transducers(geometry, mask);
    let enabled = tr_pos.len();
    let chunk = backend.max_batch(2 * foci_per_problem * enabled * 8).max(1);

    for (foci, dst) in foci.chunks(chunk).zip(dst.chunks_mut(chunk)) {
        let flat: Vec<ControlPoint> = foci.iter().flat_map(|f| f.iter().copied()).collect();
        let g = backend.batch_propagation_matrix(
            &tr_pos,
            &tr_dir,
            &flat,
            foci.len(),
            k,
            setup.directivity,
        );
        let amps = batch_target_amplitudes(backend, foci);
        let q = solve(backend, &g, &amps, foci.len(), enabled);
        quantize_batch(
            backend,
            geometry,
            &q,
            setup.constraint,
            mask,
            setup.parallel,
            dst,
        );
    }
    Ok(())
}
