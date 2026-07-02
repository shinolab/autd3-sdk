use std::num::NonZeroU8;

use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Intensity;
use autd3_rs_pattern::wavelength;
use autd3_rs_pattern_holo::{
    ControlPoint, Directivity, EmissionConstraint, GreedyOption, Pa, TransducerMask,
    abs_objective_func, greedy,
};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let mut out = geometry.pattern_buffer();

    greedy(
        &geometry,
        &[
            ControlPoint {
                point: geometry.center() + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
            ControlPoint {
                point: geometry.center() + offset(30.0 * mm, 0.0 * mm, 150.0 * mm),
                amplitude: 2.5e3 * Pa,
            },
        ],
        wavelength(340.0 * m / s),
        &GreedyOption {
            phase_quantization_levels: NonZeroU8::new(16).unwrap(),
            constraint: EmissionConstraint::Uniform(Intensity::MAX),
            directivity: Directivity::Sphere,
            objective_func: abs_objective_func,
            mask: TransducerMask::AllEnabled,
        },
        &mut out,
    )?;

    Ok(())
}
