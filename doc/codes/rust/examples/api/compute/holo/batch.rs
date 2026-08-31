use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Emission;
use autd3_rs_pattern_holo::{AmplitudeTarget, GsOption, NalgebraBackend, Pa, gs_batch};

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);
    let center = geometry.center() + offset(0.0 * mm, 0.0 * mm, 150.0 * mm);
    let wavelength = autd3_rs_pattern::wavelength(340.0 * m / s);
    let option = GsOption::default();

    // ANCHOR: api
    let problems = 64;
    let foci: Vec<AmplitudeTarget> = (0..problems)
        .map(|i| AmplitudeTarget {
            point: center + offset(i as f32 * 0.5 * mm, 0.0 * mm, 0.0 * mm),
            amplitude: 5e3 * Pa,
        })
        .collect();

    let mut dst = vec![geometry.pattern_buffer(); problems];
    gs_batch(
        &NalgebraBackend,
        &geometry,
        &foci,
        wavelength,
        &option,
        &mut dst,
    )?;
    // ANCHOR_END: api
    Ok(())
}
