use anyhow::Result;

use autd3_rs::geometry::{Autd3, Geometry, TransducerGroups, offset};
use autd3_rs::units::{m, mm, s};
use autd3_rs::value::Phase;
use autd3_rs_pattern::{focus, group, group_compute, set_phase, wavelength};
use autd3_rs_pattern_holo::{AmplitudeTarget, GspatOption, NalgebraBackend, Pa, gspat};

// ANCHOR: api
// ANCHOR: compute
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}
// ANCHOR_END: compute
// ANCHOR_END: api

fn main() -> Result<()> {
    let geometry = Geometry::new(vec![Autd3::default()]);

    let left = geometry.phase_buffer();
    let mut right = geometry.phase_buffer();
    set_phase(Phase::PI, &mut right);
    let mut dst = geometry.phase_buffer();
    let center = geometry.center();
    // ANCHOR: api
    let groups = TransducerGroups::new(&geometry, |device, tr| {
        Some(if device.position(tr).x < center.x {
            Side::Left
        } else {
            Side::Right
        })
    });
    group(
        &geometry,
        &groups,
        |side| match side {
            Side::Left => &left,
            Side::Right => &right,
        },
        Phase::ZERO,
        &mut dst,
    );
    // ANCHOR_END: api

    let wavelength = wavelength(340.0 * m / s);
    let foci = [AmplitudeTarget {
        point: center + offset(-30.0 * mm, 0.0 * mm, 150.0 * mm),
        amplitude: 5e3 * Pa,
    }];
    let target = center + offset(40.0 * mm, 0.0 * mm, 150.0 * mm);
    let mut phases = geometry.phase_buffer();
    let mut intensities = geometry.intensity_buffer();
    // ANCHOR: compute
    group_compute(
        &geometry,
        &groups,
        |side, mask, phases, intensities| match side {
            Side::Left => gspat(
                &NalgebraBackend,
                &geometry,
                &foci,
                wavelength,
                &GspatOption {
                    mask,
                    ..Default::default()
                },
                phases,
                intensities,
            ),
            Side::Right => {
                focus(&geometry, target, wavelength, phases);
                Ok(())
            }
        },
        &mut phases,
        &mut intensities,
    )?;
    // ANCHOR_END: compute
    Ok(())
}
