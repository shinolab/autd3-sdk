use std::num::NonZeroU8;

use anyhow::Result;
use rand::seq::SliceRandom;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs_pattern_holo::{
    AmplitudeTarget, Directivity, GreedyOption, Pa, abs_objective_func, greedy,
};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let foci = [
        AmplitudeTarget {
            point: center + offset(-30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
        AmplitudeTarget {
            point: center + offset(30.0 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 2.5e3 * Pa,
        },
    ];

    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let phase_quantization_levels = NonZeroU8::new(16).unwrap();
    let directivity = Directivity::Sphere;
    let objective_func = abs_objective_func;
    let option =
        // ANCHOR: option
        GreedyOption {
            phase_quantization_levels,
            directivity,
            objective_func,
        }
        // ANCHOR_END: option
        ;
    let intensities = geometry.intensity_buffer();
    let mut order: Vec<usize> = (0..geometry.num_transducers()).collect();
    order.shuffle(&mut rand::rng());
    let mut dst = geometry.phase_buffer();
    // ANCHOR: api
    greedy(
        &geometry,
        &foci,
        wavelength,
        &intensities,
        &order,
        &option,
        &mut dst,
    )?;
    // ANCHOR_END: api
    Ok(())
}
