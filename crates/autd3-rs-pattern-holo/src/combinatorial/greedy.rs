use core::f32::consts::PI;
use core::num::NonZeroU8;

use nalgebra::Complex;
use rand::seq::SliceRandom;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::{Geometry, TransducerMask};
use autd3_rs_core::value::{Intensity, PatternIntensity, Phase};

use crate::amp::Amplitude;
use crate::amplitude_target::AmplitudeTarget;
use crate::directivity::Directivity;
use crate::error::HoloError;
use crate::propagation::{propagate, validate_dst_len, wavenumber};

#[must_use]
pub fn abs_objective_func(c: Complex<f32>, a: Amplitude) -> f32 {
    (a.pascal() - c.norm()).abs()
}

#[derive(Debug, Clone, Copy)]
pub struct GreedyOption<'a> {
    pub phase_quantization_levels: NonZeroU8,
    pub directivity: Directivity,
    pub objective_func: fn(Complex<f32>, Amplitude) -> f32,
    pub mask: TransducerMask<'a>,
}

impl Default for GreedyOption<'_> {
    fn default() -> Self {
        Self {
            phase_quantization_levels: NonZeroU8::new(16).unwrap(),
            directivity: Directivity::Sphere,
            objective_func: abs_objective_func,
            mask: TransducerMask::AllEnabled,
        }
    }
}

fn validate_intensities(
    intensities: PatternIntensity<'_>,
    geometry: &Geometry,
) -> Result<(), HoloError> {
    let PatternIntensity::PerDevice(intensities) = intensities else {
        return Ok(());
    };
    if intensities.len() != geometry.num_devices() {
        return Err(HoloError::IntensityDeviceCountMismatch {
            got: intensities.len(),
            expected: geometry.num_devices(),
        });
    }
    for (device, (slot, dev)) in intensities.iter().zip(geometry.iter()).enumerate() {
        if slot.len() != dev.num_transducers() {
            return Err(HoloError::IntensityTransducerCountMismatch {
                device,
                got: slot.len(),
                expected: dev.num_transducers(),
            });
        }
    }
    Ok(())
}

fn intensity_at(intensities: PatternIntensity<'_>, d: usize, t: usize) -> Intensity {
    match intensities {
        PatternIntensity::Uniform(intensity) => intensity,
        PatternIntensity::PerDevice(intensities) => intensities[d][t],
    }
}

#[allow(clippy::many_single_char_names)]
pub fn greedy<'a>(
    geometry: &Geometry,
    foci: &[AmplitudeTarget],
    wavelength: Length,
    intensities: impl Into<PatternIntensity<'a>>,
    option: &GreedyOption<'_>,
    dst: &mut [Vec<Phase>],
) -> Result<(), HoloError> {
    let intensities = intensities.into();
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    validate_dst_len(dst.len(), geometry)?;
    validate_intensities(intensities, geometry)?;
    let mask = option.mask;
    mask.validate(geometry)?;

    let k = wavenumber(wavelength);
    let m = foci.len();
    let levels = option.phase_quantization_levels.get();

    let phase_candidates: Vec<Complex<f32>> = (0..levels)
        .map(|i| Complex::new(0.0, 2.0 * PI * f32::from(i) / f32::from(levels)).exp())
        .collect();

    let mut indices: Vec<(usize, usize)> = geometry
        .iter()
        .enumerate()
        .flat_map(|(d, dev)| {
            (0..dev.num_transducers())
                .filter(move |&t| {
                    mask.is_enabled(d, t) && intensity_at(intensities, d, t) != Intensity::MIN
                })
                .map(move |t| (d, t))
        })
        .collect();
    indices.shuffle(&mut rand::rng());

    for slot in dst.iter_mut() {
        slot.fill(Phase::ZERO);
    }

    let mut cache = vec![Complex::new(0.0, 0.0); m];
    let mut tmp = vec![Complex::new(0.0, 0.0); m];

    for &(d, t) in &indices {
        let dev = &geometry[d];
        let pos = dev.positions()[t];
        let dir = dev.directions()[t];
        let amp = f32::from(intensity_at(intensities, d, t).0) / f32::from(Intensity::MAX.0);
        for (r, f) in tmp.iter_mut().zip(foci) {
            *r = propagate(pos, dir, f.point, k, option.directivity) * amp;
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

        dst[d][t] = Phase::from(best_phase);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use autd3_rs_core::geometry::{Autd3, Point3, TransducerMaskError, Vector3};

    use super::*;
    use crate::Pa;
    use crate::test_utils::{geometry, wavelength};

    fn single_focus() -> [AmplitudeTarget; 1] {
        [AmplitudeTarget {
            point: Point3::origin() + Vector3::new(0.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        }]
    }

    fn uniform(geometry: &Geometry, intensity: Intensity) -> Vec<Vec<Intensity>> {
        let mut intensities = geometry.intensity_buffer();
        for slot in &mut intensities {
            slot.fill(intensity);
        }
        intensities
    }

    fn field_at(
        geometry: &Geometry,
        target: Point3<f32>,
        intensities: &[Vec<Intensity>],
        phases: &[Vec<Phase>],
    ) -> Complex<f32> {
        let k = wavenumber(wavelength());
        geometry
            .iter()
            .enumerate()
            .flat_map(|(d, dev)| (0..dev.num_transducers()).map(move |t| (d, dev, t)))
            .map(|(d, dev, t)| {
                let amp = f32::from(intensities[d][t].0) / f32::from(Intensity::MAX.0);
                propagate(
                    dev.positions()[t],
                    dev.directions()[t],
                    target,
                    k,
                    Directivity::Sphere,
                ) * amp
                    * Complex::new(0.0, phases[d][t].rad()).exp()
            })
            .sum()
    }

    #[test]
    fn empty_foci_is_error() {
        let geometry = geometry(1);
        let intensities = uniform(&geometry, Intensity::MAX);
        let mut dst = geometry.phase_buffer();
        assert_eq!(
            greedy(
                &geometry,
                &[],
                wavelength(),
                &intensities,
                &GreedyOption::default(),
                &mut dst
            ),
            Err(HoloError::NoFoci)
        );
    }

    #[test]
    fn a_mask_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let geometry = geometry(2);
        let intensities = uniform(&geometry, Intensity::MAX);
        let mut dst = geometry.phase_buffer();

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
                &intensities,
                &option,
                &mut dst
            ),
            Err(HoloError::Mask(TransducerMaskError::DeviceCountMismatch {
                got: 1,
                expected: 2
            })),
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
                &intensities,
                &option,
                &mut dst
            ),
            Err(HoloError::Mask(
                TransducerMaskError::TransducerCountMismatch {
                    device: 1,
                    got: 3,
                    expected: Autd3::NUM_TRANSDUCERS,
                }
            )),
        );
    }

    #[test]
    fn a_dst_that_does_not_match_the_geometry_is_an_error_not_a_panic() {
        let geometry = geometry(2);
        let intensities = uniform(&geometry, Intensity::MAX);
        let mut dst = vec![vec![Phase::ZERO; Autd3::NUM_TRANSDUCERS]];
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &intensities,
                &GreedyOption::default(),
                &mut dst
            ),
            Err(HoloError::DstDeviceCountMismatch {
                got: 1,
                expected: 2
            }),
        );
    }

    #[test]
    fn intensities_that_do_not_match_the_geometry_are_an_error_not_a_panic() {
        let geometry = geometry(2);
        let mut dst = geometry.phase_buffer();

        let one_device = vec![vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS]];
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &one_device,
                &GreedyOption::default(),
                &mut dst
            ),
            Err(HoloError::IntensityDeviceCountMismatch {
                got: 1,
                expected: 2
            }),
        );

        let short_row = vec![
            vec![Intensity::MAX; Autd3::NUM_TRANSDUCERS],
            vec![Intensity::MAX; 3],
        ];
        assert_eq!(
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                &short_row,
                &GreedyOption::default(),
                &mut dst
            ),
            Err(HoloError::IntensityTransducerCountMismatch {
                device: 1,
                got: 3,
                expected: Autd3::NUM_TRANSDUCERS,
            }),
        );
    }

    #[test]
    fn uniform_intensities_focus() {
        let geometry = geometry(1);
        let intensities = uniform(&geometry, Intensity::MAX);
        let mut dst = geometry.phase_buffer();
        greedy(
            &geometry,
            &single_focus(),
            wavelength(),
            &intensities,
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();
        assert!(dst[0].iter().any(|&p| p != dst[0][0]));
    }

    #[test]
    fn the_given_intensities_scale_the_reached_amplitude() {
        let geometry = geometry(1);
        let focus = [AmplitudeTarget {
            amplitude: 1e3 * Pa,
            ..single_focus()[0]
        }];
        let weak = uniform(&geometry, Intensity(64));
        let mut dst = geometry.phase_buffer();
        greedy(
            &geometry,
            &focus,
            wavelength(),
            &weak,
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();

        let reached = field_at(&geometry, focus[0].point, &weak, &dst).norm();
        let target = focus[0].amplitude.pascal();
        assert!(
            (reached - target).abs() < target * 0.05,
            "reached {reached} Pa for a {target} Pa target"
        );
    }

    #[test]
    fn a_uniform_intensity_applies_to_every_transducer() {
        let geometry = geometry(1);
        let foci = [AmplitudeTarget {
            amplitude: 1e3 * Pa,
            ..single_focus()[0]
        }];
        let mut from_uniform = geometry.phase_buffer();
        greedy(
            &geometry,
            &foci,
            wavelength(),
            Intensity(64),
            &GreedyOption::default(),
            &mut from_uniform,
        )
        .unwrap();
        let reached = field_at(
            &geometry,
            foci[0].point,
            &uniform(&geometry, Intensity(64)),
            &from_uniform,
        )
        .norm();
        let target = foci[0].amplitude.pascal();
        assert!(
            (reached - target).abs() < target * 0.05,
            "reached {reached} Pa for a {target} Pa target"
        );
    }

    #[test]
    fn silent_transducers_are_left_out_of_the_search() {
        let geometry = geometry(1);
        let mut intensities = uniform(&geometry, Intensity::MAX);
        for i in &mut intensities[0][..100] {
            *i = Intensity::MIN;
        }
        let given = intensities.clone();
        let mut dst = geometry.phase_buffer();
        for slot in &mut dst {
            slot.fill(Phase(0x80));
        }
        greedy(
            &geometry,
            &single_focus(),
            wavelength(),
            &intensities,
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();
        assert_eq!(intensities, given);
        assert!(dst[0][..100].iter().all(|&p| p == Phase::ZERO));
        assert!(dst[0][100..].iter().any(|&p| p != Phase::ZERO));
    }
}
