use core::f32::consts::PI;
use core::num::NonZeroU8;

use nalgebra::Complex;
use rand::seq::SliceRandom;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::params::NUM_TRANSDUCERS;
use autd3_rs_core::value::{Emission, Intensity, Phase};

use crate::amp::Amplitude;
use crate::constraint::EmissionConstraint;
use crate::control_point::ControlPoint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::propagate;

#[must_use]
pub fn abs_objective_func(c: Complex<f32>, a: Amplitude) -> f32 {
    (a.pascal() - c.norm()).abs()
}

#[derive(Debug, Clone, Copy)]
pub struct GreedyOption {
    pub phase_quantization_levels: NonZeroU8,
    pub constraint: EmissionConstraint,
    pub directivity: Directivity,
    pub objective_func: fn(Complex<f32>, Amplitude) -> f32,
}

impl Default for GreedyOption {
    fn default() -> Self {
        Self {
            phase_quantization_levels: NonZeroU8::new(16).unwrap(),
            constraint: EmissionConstraint::Uniform(Intensity::MAX),
            directivity: Directivity::Sphere,
            objective_func: abs_objective_func,
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn greedy(
    geometry: &Geometry,
    foci: &[ControlPoint],
    wavelength: Length,
    option: &GreedyOption,
    mask: TransducerMask<'_>,
    out: &mut [[Emission; NUM_TRANSDUCERS]],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    assert_eq!(
        out.len(),
        geometry.len(),
        "out must have one slot per device"
    );
    mask.validate(geometry);

    let wavenumber = 2.0 * PI / wavelength.mm();
    let m = foci.len();
    let levels = option.phase_quantization_levels.get();

    let phase_candidates: Vec<Complex<f32>> = (0..levels)
        .map(|i| Complex::new(0.0, 2.0 * PI * f32::from(i) / f32::from(levels)).exp())
        .collect();

    let devices: Vec<&Device> = geometry.iter().collect();
    let mut indices: Vec<(usize, usize)> = devices
        .iter()
        .enumerate()
        .flat_map(|(d, dev)| {
            (0..dev.len())
                .filter(move |&t| mask.is_enabled(d, t))
                .map(move |t| (d, t))
        })
        .collect();
    indices.shuffle(&mut rand::rng());

    for slot in out.iter_mut() {
        *slot = [Emission::default(); NUM_TRANSDUCERS];
    }

    let intensity = option.constraint.convert(1.0, 1.0);

    let mut cache = vec![Complex::new(0.0, 0.0); m];
    let mut tmp = vec![Complex::new(0.0, 0.0); m];

    for &(d, t) in &indices {
        let dev = devices[d];
        let pos = dev.positions()[t];
        let dir = dev.directions()[t];
        for (r, f) in tmp.iter_mut().zip(foci) {
            *r = propagate(pos, dir, f.point, wavenumber, option.directivity);
        }

        let mut best_phase = Complex::new(0.0, 0.0);
        let mut best_value = f32::INFINITY;
        for &phase in &phase_candidates {
            let value = cache
                .iter()
                .zip(foci)
                .zip(&tmp)
                .fold(0.0, |acc, ((c, f), trans)| {
                    acc + (option.objective_func)(trans * phase + c, f.amplitude)
                });
            if value < best_value {
                best_value = value;
                best_phase = phase;
            }
        }

        for (c, trans) in cache.iter_mut().zip(&tmp) {
            *c += trans * best_phase;
        }

        out[d][t] = Emission {
            phase: Phase::from(best_phase),
            intensity,
        };
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::common::units::{m, s};
    use autd3_rs_core::geometry::{Autd3, Geometry, Point3, Vector3};

    use super::*;
    use crate::Pa;

    fn wavelength() -> Length {
        autd3_rs_pattern::wavelength(340.0 * m / s)
    }

    fn single_focus() -> [ControlPoint; 1] {
        [ControlPoint {
            point: Point3::origin() + Vector3::new(0.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        }]
    }

    fn buffer(geometry: &Geometry) -> Vec<[Emission; NUM_TRANSDUCERS]> {
        vec![[Emission::default(); NUM_TRANSDUCERS]; geometry.len()]
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let mut out = buffer(&geometry);
        assert_eq!(
            greedy(
                &geometry,
                &[],
                wavelength(),
                &GreedyOption::default(),
                TransducerMask::AllEnabled,
                &mut out
            ),
            Err(HoloError::NoFoci)
        );
    }

    #[test]
    fn uniform_default_sets_all_max_and_focuses() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let mut out = buffer(&geometry);
        greedy(
            &geometry,
            &single_focus(),
            wavelength(),
            &GreedyOption::default(),
            TransducerMask::AllEnabled,
            &mut out,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].iter().all(|e| e.intensity == Intensity::MAX));
        assert!(out[0].iter().any(|e| e.phase != out[0][0].phase));
    }
}
