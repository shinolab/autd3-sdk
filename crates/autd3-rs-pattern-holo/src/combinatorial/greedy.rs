use core::f32::consts::PI;
use core::num::NonZeroU8;

use nalgebra::Complex;
use rand::seq::SliceRandom;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Device, Geometry};
use autd3_rs_core::value::{Intensity, Phase};

use crate::amp::Amplitude;
use crate::amplitude_target::AmplitudeTarget;
use crate::constraint::IntensityConstraint;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::mask::TransducerMask;
use crate::propagation::propagate;

#[must_use]
pub fn abs_objective_func(c: Complex<f32>, a: Amplitude) -> f32 {
    (a.pascal() - c.norm()).abs()
}

#[derive(Debug, Clone, Copy)]
pub struct GreedyOption<'a> {
    pub phase_quantization_levels: NonZeroU8,
    pub constraint: IntensityConstraint,
    pub directivity: Directivity,
    pub objective_func: fn(Complex<f32>, Amplitude) -> f32,
    pub mask: TransducerMask<'a>,
}

impl Default for GreedyOption<'_> {
    fn default() -> Self {
        Self {
            phase_quantization_levels: NonZeroU8::new(16).unwrap(),
            constraint: IntensityConstraint::Uniform(Intensity::MAX),
            directivity: Directivity::Sphere,
            objective_func: abs_objective_func,
            mask: TransducerMask::AllEnabled,
        }
    }
}

#[allow(clippy::many_single_char_names)]
pub fn greedy(
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    option: &GreedyOption<'_>,
    phases: &mut [Vec<Phase>],
    intensities: &mut [Vec<Intensity>],
) -> Result<(), HoloError> {
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    crate::mask::validate_dst_len(phases.len(), geometry)?;
    crate::mask::validate_dst_len(intensities.len(), geometry)?;
    let mask = option.mask;
    mask.validate(geometry)?;

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
            (0..dev.num_transducers())
                .filter(move |&t| mask.is_enabled(d, t))
                .map(move |t| (d, t))
        })
        .collect();
    indices.shuffle(&mut rand::rng());

    for slot in phases.iter_mut() {
        slot.fill(Phase::ZERO);
    }
    for slot in intensities.iter_mut() {
        slot.fill(Intensity::MIN);
    }

    let intensity = option.constraint.convert(1.0, 1.0);
    let amp = f32::from(intensity.0) / f32::from(Intensity::MAX.0);

    let mut cache = vec![Complex::new(0.0, 0.0); m];
    let mut tmp = vec![Complex::new(0.0, 0.0); m];

    for &(d, t) in &indices {
        let dev = devices[d];
        let pos = dev.positions()[t];
        let dir = dev.directions()[t];
        for (r, f) in tmp.iter_mut().zip(foci) {
            *r = propagate(pos, dir, f.point, wavenumber, option.directivity) * amp;
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

        phases[d][t] = Phase::from(best_phase);
        intensities[d][t] = intensity;
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

    fn single_focus() -> [AmplitudeTarget; 1] {
        [AmplitudeTarget {
            point: Point3::origin() + Vector3::new(0.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        }]
    }

    fn buffer(geometry: &Geometry) -> (Vec<Vec<Phase>>, Vec<Vec<Intensity>>) {
        (geometry.phase_buffer(), geometry.intensity_buffer())
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let (mut phases, mut intensities) = buffer(&geometry);
        assert_eq!(
            greedy(
                &geometry,
                &[],
                wavelength(),
                &GreedyOption::default(),
                &mut phases,
                &mut intensities
            ),
            Err(HoloError::NoFoci)
        );
    }

    #[test]
    fn a_mask_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let (mut phases, mut intensities) = buffer(&geometry);

        let one_device = vec![vec![true; Autd3::NUM_TRANSDUCERS]];
        let option = GreedyOption {
            mask: TransducerMask::Masked(&one_device),
            ..GreedyOption::default()
        };
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &option,
                &mut phases,
                &mut intensities
            ),
            Err(HoloError::MaskDeviceCountMismatch {
                got: 1,
                expected: 2
            }),
        );

        let short_row = vec![vec![true; Autd3::NUM_TRANSDUCERS], vec![true; 3]];
        let option = GreedyOption {
            mask: TransducerMask::Masked(&short_row),
            ..GreedyOption::default()
        };
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &option,
                &mut phases,
                &mut intensities
            ),
            Err(HoloError::MaskTransducerCountMismatch {
                device: 1,
                got: 3,
                expected: Autd3::NUM_TRANSDUCERS,
            }),
        );
    }

    #[test]
    fn a_dst_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let geometry = Geometry::new(vec![Autd3::default(), Autd3::default()]);
        let mut phases = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        let mut intensities = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &GreedyOption::default(),
                &mut phases,
                &mut intensities
            ),
            Err(HoloError::DstDeviceCountMismatch {
                got: 1,
                expected: 2
            }),
        );
    }

    #[test]
    fn uniform_default_sets_all_max_and_focuses() {
        let geometry = Geometry::new(vec![Autd3::default()]);
        let (mut phases, mut intensities) = buffer(&geometry);
        greedy(
            &geometry,
            &single_focus(),
            wavelength(),
            &GreedyOption::default(),
            &mut phases,
            &mut intensities,
        )
        .unwrap();
        assert_eq!(phases.len(), 1);
        assert!(intensities[0].iter().all(|&i| i == Intensity::MAX));
        assert!(phases[0].iter().any(|&p| p != phases[0][0]));
    }
}
