use core::f32::consts::PI;
use core::num::NonZeroU8;

use nalgebra::Complex;

use autd3_rs_core::common::Length;
use autd3_rs_core::geometry::Geometry;
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

#[derive(Debug, Clone)]
pub struct GreedyOption {
    pub phase_quantization_levels: NonZeroU8,
    pub directivity: Directivity,
    pub objective_func: fn(Complex<f32>, Amplitude) -> f32,
}

impl Default for GreedyOption {
    fn default() -> Self {
        Self {
            phase_quantization_levels: NonZeroU8::new(16).unwrap(),
            directivity: Directivity::Sphere,
            objective_func: abs_objective_func,
        }
    }
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
    order: &[usize],
    option: &GreedyOption,
    dst: &mut [Vec<Phase>],
) -> Result<(), HoloError> {
    let intensities = intensities.into();
    if foci.is_empty() {
        return Err(HoloError::NoFoci);
    }
    validate_dst_len(dst.len(), geometry)?;
    if let PatternIntensity::PerDevice(intensities) = intensities
        && intensities.len() != geometry.num_devices()
    {
        return Err(HoloError::IntensityDeviceCountMismatch {
            got: intensities.len(),
            expected: geometry.num_devices(),
        });
    }

    let k = wavenumber(wavelength);
    let m = foci.len();
    let levels = option.phase_quantization_levels.get();

    let phase_candidates: Vec<Complex<f32>> = (0..levels)
        .map(|i| Complex::new(0.0, 2.0 * PI * f32::from(i) / f32::from(levels)).exp())
        .collect();

    let mut device_offsets = Vec::with_capacity(geometry.num_devices());
    let mut device_directions = Vec::with_capacity(geometry.num_devices());
    let mut offset = 0;
    for (device, (dev, slot)) in geometry.iter().zip(dst.iter_mut()).enumerate() {
        if let PatternIntensity::PerDevice(intensities) = intensities
            && intensities[device].len() != dev.num_transducers()
        {
            return Err(HoloError::IntensityTransducerCountMismatch {
                device,
                got: intensities[device].len(),
                expected: dev.num_transducers(),
            });
        }
        device_offsets.push(offset);
        device_directions.push(dev.axial_direction());
        offset += dev.num_transducers();
        slot.fill(Phase::ZERO);
    }

    let mut cache = vec![Complex::new(0.0, 0.0); m];
    let mut tmp = vec![Complex::new(0.0, 0.0); m];

    for &index in order {
        let d = device_offsets.partition_point(|&start| start <= index) - 1;
        let t = index - device_offsets[d];
        if intensity_at(intensities, d, t) == Intensity::MIN {
            continue;
        }
        let pos = geometry[d].positions()[t];
        let dir = device_directions[d];
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
    use autd3_rs_core::geometry::{Autd3, Point3, Vector3};

    use super::*;
    use crate::Pa;
    use crate::test_utils::{geometry, wavelength};

    fn single_focus() -> [AmplitudeTarget; 1] {
        [AmplitudeTarget {
            point: Point3::origin() + Vector3::new(0.0, 0.0, 150.0),
            amplitude: 5e3 * Pa,
        }]
    }

    fn ascending(geometry: &Geometry) -> Vec<usize> {
        (0..geometry.num_transducers()).collect()
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
                    dev.axial_direction(),
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
                &ascending(&geometry),
                &GreedyOption::default(),
                &mut dst
            ),
            Err(HoloError::NoFoci)
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
                &ascending(&geometry),
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
                &ascending(&geometry),
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
                &ascending(&geometry),
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
            &ascending(&geometry),
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
            &ascending(&geometry),
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();

        let reached = field_at(&geometry, focus[0].point, &weak, &dst).norm();
        let target = focus[0].amplitude.pascal();
        approx::assert_relative_eq!(reached, target, max_relative = 0.05);
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
            &ascending(&geometry),
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
        approx::assert_relative_eq!(reached, target, max_relative = 0.05);
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
            &ascending(&geometry),
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();
        assert_eq!(intensities, given);
        assert!(dst[0][..100].iter().all(|&p| p == Phase::ZERO));
        assert!(dst[0][100..].iter().any(|&p| p != Phase::ZERO));
    }

    #[test]
    fn the_same_order_gives_the_same_phases() {
        let geometry = geometry(2);
        let order: Vec<usize> = (0..geometry.num_transducers())
            .map(|i| (i * 7) % geometry.num_transducers())
            .collect();
        let run = || {
            let mut dst = geometry.phase_buffer();
            greedy(
                &geometry,
                &single_focus(),
                wavelength(),
                Intensity::MAX,
                &order,
                &GreedyOption::default(),
                &mut dst,
            )
            .unwrap();
            dst
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn indices_past_the_first_device_reach_the_later_device() {
        let geometry = geometry(2);
        let mut intensities = uniform(&geometry, Intensity::MAX);
        intensities[0].fill(Intensity::MIN);
        let reversed: Vec<usize> = (0..geometry.num_transducers()).rev().collect();
        let mut dst = geometry.phase_buffer();
        greedy(
            &geometry,
            &single_focus(),
            wavelength(),
            &intensities,
            &reversed,
            &GreedyOption::default(),
            &mut dst,
        )
        .unwrap();
        assert!(dst[0].iter().all(|&p| p == Phase::ZERO));
        assert!(dst[1].iter().any(|&p| p != Phase::ZERO));
    }
}
